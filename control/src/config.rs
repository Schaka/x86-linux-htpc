// SPDX-License-Identifier: AGPL-3.0-only
//! The configuration file of the HTPC. The file is KEY=value lines and is
//! parsed as data, with the same rules as config_get in lib/htpc.sh.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG: &str = "/etc/x86-linux-htpc/htpc.conf";
pub const DEFAULT_PORT: u16 = 8780;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub dlna: bool,
    /// The name in Home Assistant and in the DLNA apps
    pub name: String,
    /// The desktop IDs of the home screen, in their order
    pub home_screen_apps: Vec<String>,
    /// system or cb1, as JELLYFIN_MPV
    pub jellyfin_mpv: String,
    pub home: PathBuf,
}

impl Config {
    pub fn load(path: &Path) -> Config {
        let values = std::fs::read_to_string(path)
            .map(|text| parse(&text))
            .unwrap_or_default();
        let get = |key: &str| values.get(key).cloned().unwrap_or_default();
        let name = [
            get("CONTROL_NAME"),
            get("JELLYFIN_PLAYER_NAME"),
            host_name(),
        ]
        .into_iter()
        .find(|n| !n.is_empty())
        .unwrap_or_else(|| "HTPC".into());
        Config {
            port: get("CONTROL_API_PORT").parse().unwrap_or(DEFAULT_PORT),
            dlna: get("CONTROL_DLNA") != "no",
            name,
            home_screen_apps: get("HOME_SCREEN_APPS")
                .split_whitespace()
                .map(String::from)
                .collect(),
            jellyfin_mpv: Some(get("JELLYFIN_MPV"))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "system".into()),
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/".into()),
        }
    }
}

/// The last line of a key wins, and one pair of double quotes around the
/// value is removed.
fn parse(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let value = value.strip_suffix('"').unwrap_or(value);
            let value = value.strip_prefix('"').unwrap_or(value);
            values.insert(key.to_string(), value.to_string());
        }
    }
    values
}

fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_config_get() {
        let values = parse("# A=1\nA=2\nB=\"x y\"\n  C=3\nA=4\nbad line\n");
        assert_eq!(values["A"], "4");
        assert_eq!(values["B"], "x y");
        assert_eq!(values["C"], "3");
        assert!(!values.contains_key("bad line"));
    }
}
