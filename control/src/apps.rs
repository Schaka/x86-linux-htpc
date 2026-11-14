// SPDX-License-Identifier: AGPL-3.0-only
//! The apps of the home screen, and their start.
//!
//! The apps are the desktop entries of HOME_SCREEN_APPS, or else the
//! entries that the components install. An app starts as a transient
//! systemd user service, the same way the home screen of Plasma starts it.
//! So the app keeps running when this service restarts.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;
use utoipa::ToSchema;

use crate::config::Config;
use crate::error::{ApiError, Result};

pub const YOUTUBE_ENTRY: &str = "htpc-youtube-tv.desktop";
pub const KODI_ENTRY: &str = "htpc-kodi.desktop";
pub const JELLYFIN_ENTRY: &str = "com.github.iwalton3.jellyfin-mpv-shim.desktop";
const ANDROID_PREFIX: &str = "htpc-android-";

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct App {
    /// The name for the API, for example youtube, kodi or s0undtv
    pub id: String,
    /// The name on the home screen
    pub name: String,
    pub desktop_id: String,
}

fn application_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
        "/var/lib/flatpak/exports/share/applications".into(),
        "/usr/local/share/applications".into(),
        "/usr/share/applications".into(),
    ]
}

fn find_entry(home: &Path, desktop_id: &str) -> Option<PathBuf> {
    application_dirs(home)
        .into_iter()
        .map(|d| d.join(desktop_id))
        .find(|p| p.is_file())
}

/// The keys of the group [Desktop Entry]
fn read_entry(path: &Path) -> Option<Vec<(String, String)>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_group = false;
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
        } else if in_group && let Some((key, value)) = line.split_once('=') {
            keys.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    Some(keys)
}

fn entry_value(keys: &[(String, String)], key: &str) -> Option<String> {
    keys.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

pub fn app_id(desktop_id: &str) -> String {
    let stem = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    match desktop_id {
        YOUTUBE_ENTRY => "youtube".into(),
        KODI_ENTRY => "kodi".into(),
        JELLYFIN_ENTRY => "jellyfin".into(),
        _ => stem
            .strip_prefix(ANDROID_PREFIX)
            .or(stem.strip_prefix("htpc-"))
            .unwrap_or(stem)
            .to_string(),
    }
}

pub fn list(config: &Config) -> Vec<App> {
    let mut ids = config.home_screen_apps.clone();
    if ids.is_empty() {
        ids = vec![
            JELLYFIN_ENTRY.into(),
            YOUTUBE_ENTRY.into(),
            KODI_ENTRY.into(),
        ];
        let mut android: Vec<String> =
            std::fs::read_dir(config.home.join(".local/share/applications"))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.starts_with(ANDROID_PREFIX) && n.ends_with(".desktop"))
                .collect();
        android.sort();
        ids.extend(android);
    }
    ids.into_iter()
        .filter_map(|desktop_id| {
            let keys = read_entry(&find_entry(&config.home, &desktop_id)?)?;
            Some(App {
                id: app_id(&desktop_id),
                name: entry_value(&keys, "Name").unwrap_or_else(|| desktop_id.clone()),
                desktop_id,
            })
        })
        .collect()
}

pub fn find(config: &Config, id: &str) -> Option<App> {
    list(config)
        .into_iter()
        .find(|a| a.id == id || a.desktop_id == id)
}

/// Starts a desktop entry. The files take the place of the field codes
/// %f, %F, %u and %U.
pub async fn launch(config: &Config, desktop_id: &str, files: &[String]) -> Result<()> {
    let path = find_entry(&config.home, desktop_id).ok_or_else(|| {
        ApiError::not_installed(format!("The app {desktop_id} is not installed."))
    })?;
    let keys = read_entry(&path)
        .ok_or_else(|| ApiError::not_installed(format!("Cannot read {}.", path.display())))?;
    let exec = entry_value(&keys, "Exec")
        .ok_or_else(|| ApiError::not_installed(format!("{desktop_id} has no Exec line.")))?;
    let argv = expand_exec(&exec, files);
    if argv.is_empty() {
        return Err(ApiError::not_installed(format!(
            "{desktop_id} has an empty Exec line."
        )));
    }
    run_detached(&format!("app-htpc-control-{}", app_id(desktop_id)), &argv)
        .await
        .map(|_| ())
}

/// Runs a program as a transient user service, and returns the name of
/// the service.
pub async fn run_detached(unit_prefix: &str, argv: &[String]) -> Result<String> {
    let unit = format!(
        "{unit_prefix}-{}",
        std::process::id() as u64 * 1000 + unique()
    );
    let output = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--unit", &unit, "--"])
        .args(argv)
        .output()
        .await?;
    if !output.status.success() {
        return Err(ApiError::backend(format!(
            "systemd-run failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    tracing::info!("started {}", argv.join(" "));
    Ok(unit)
}

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed) % 1000
}

/// Splits an Exec value into arguments, with the quoting rules of the
/// desktop entry specification, and fills in the field codes.
fn expand_exec(exec: &str, files: &[String]) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    let mut argv = Vec::new();
    for word in words {
        match word.as_str() {
            "%f" | "%u" => argv.extend(files.first().cloned()),
            "%F" | "%U" => argv.extend(files.iter().cloned()),
            "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            _ => argv.push(word.replace("%%", "%")),
        }
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_exec_lines() {
        let files = vec!["https://www.youtube.com/watch?v=x".to_string()];
        assert_eq!(
            expand_exec("/usr/bin/app %U", &files),
            vec!["/usr/bin/app", "https://www.youtube.com/watch?v=x"]
        );
        assert_eq!(
            expand_exec("app --name \"two words\" %i", &[]),
            vec!["app", "--name", "two words"]
        );
        assert_eq!(
            expand_exec("android-app com.example", &[]),
            vec!["android-app", "com.example"]
        );
    }

    #[test]
    fn names_apps() {
        assert_eq!(app_id("htpc-android-s0undtv.desktop"), "s0undtv");
        assert_eq!(app_id(YOUTUBE_ENTRY), "youtube");
        assert_eq!(app_id("org.mozilla.firefox.desktop"), "org.mozilla.firefox");
    }
}
