// SPDX-License-Identifier: AGPL-3.0-only
//! The configuration file of the HTPC. The file is KEY=value lines and is
//! parsed as data, with the same rules as config_get in lib/htpc.sh.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG: &str = "/etc/x86-linux-htpc/htpc.conf";
pub const DEFAULT_PORT: u16 = 8780;
const DEFAULT_NAME: &str = "HTPC";

/// The mpv program of jellyfin-mpv-shim, from JELLYFIN_MPV
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MpvBuild {
    /// The mpv package of the distribution
    #[default]
    System,
    /// The mpv with the CB1 Dolby Vision engine, on its own VT
    Cb1,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub dlna: bool,
    /// Whether phones find VacuumTube with DIAL, from CONTROL_DIAL
    pub dial: bool,
    /// The name in Home Assistant and in the DLNA apps
    pub name: String,
    /// The desktop IDs of the home screen, in their order
    pub home_screen_apps: Vec<String>,
    pub jellyfin_mpv: MpvBuild,
    pub home: PathBuf,
    /// The folder for the sockets and scripts of this service
    pub runtime_dir: PathBuf,
}

impl Config {
    /// A missing file gives the defaults, because the service also runs
    /// before the setup writes the file.
    pub fn load(path: &Path) -> Config {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                if error.kind() != ErrorKind::NotFound {
                    tracing::warn!("cannot read {}: {error}", path.display());
                }
                String::new()
            }
        };
        Config::from_values(&parse(&text))
    }

    fn from_values(values: &HashMap<String, String>) -> Config {
        let get = |key: &str| values.get(key).map(String::as_str).unwrap_or_default();
        let name = [get("CONTROL_NAME"), get("JELLYFIN_PLAYER_NAME")]
            .into_iter()
            .find(|name| !name.is_empty())
            .map(String::from)
            .or_else(host_name)
            .unwrap_or_else(|| DEFAULT_NAME.into());
        Config {
            port: get("CONTROL_API_PORT").parse().unwrap_or(DEFAULT_PORT),
            dlna: get("CONTROL_DLNA") != "no",
            dial: get("CONTROL_DIAL") != "no",
            name,
            home_screen_apps: get("HOME_SCREEN_APPS")
                .split_whitespace()
                .map(String::from)
                .collect(),
            jellyfin_mpv: match get("JELLYFIN_MPV") {
                "cb1" => MpvBuild::Cb1,
                _ => MpvBuild::System,
            },
            home: env_path("HOME").unwrap_or_else(|| "/".into()),
            runtime_dir: env_path("XDG_RUNTIME_DIR").unwrap_or_else(std::env::temp_dir),
        }
    }
}

#[cfg(test)]
impl Config {
    /// The defaults, with a home folder for the files of a test
    pub fn for_test(home: PathBuf) -> Config {
        let mut config = Config::from_values(&HashMap::new());
        config.name = "Test HTPC".into();
        config.runtime_dir = home.clone();
        config.home = home;
        config
    }
}

/// The last line of a key wins, and one pair of double quotes around the
/// value is removed.
fn parse(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| valid_key(key))
        .map(|(key, value)| {
            let value = value.strip_suffix('"').unwrap_or(value);
            let value = value.strip_prefix('"').unwrap_or(value);
            (key.to_string(), value.to_string())
        })
        .collect()
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn host_name() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
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

    #[test]
    fn reads_the_values() {
        let config = Config::from_values(&parse(
            "CONTROL_NAME=Living room\nCONTROL_API_PORT=9000\nCONTROL_DLNA=no\nCONTROL_DIAL=no\nJELLYFIN_MPV=cb1\n",
        ));
        assert_eq!(config.name, "Living room");
        assert_eq!(config.port, 9000);
        assert!(!config.dlna);
        assert!(!config.dial);
        assert_eq!(config.jellyfin_mpv, MpvBuild::Cb1);
    }

    #[test]
    fn uses_defaults_for_bad_values() {
        let config = Config::from_values(&parse("CONTROL_API_PORT=many\nJELLYFIN_MPV=other\n"));
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(config.dlna);
        assert!(config.dial);
        assert_eq!(config.jellyfin_mpv, MpvBuild::System);
    }
}
