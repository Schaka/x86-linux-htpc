// SPDX-License-Identifier: AGPL-3.0-only
//! Plays a media address that no app of the HTPC knows, for example a
//! file of a DLNA server, in mpv.
//!
//! mpv starts with the configuration folder of jellyfin-mpv-shim. So it
//! uses the same mpv program, options and scripts as the shim: the CB1 mpv
//! on its own VT with JELLYFIN_MPV=cb1, the refresh rate script, and the
//! TV wake. This service controls it over the JSON IPC of mpv.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;

use crate::apps::run_detached;
use crate::config::Config;
use crate::error::{ApiError, Result};
use crate::status::{PlayState, Running, Source, Status};

const CB1_MPV: &str = "/opt/x86-linux-htpc/cb1/bin/mpv";
const TIMEOUT: Duration = Duration::from_secs(2);
/// The VT of the CB1 mpv, as in the component jellyfin-mpv-shim
const CB1_VT: &str = "tty8";

pub struct Mpv {
    socket: PathBuf,
    program: String,
    config_dir: PathBuf,
}

impl Mpv {
    pub fn new(config: &Config) -> Mpv {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let program = if config.jellyfin_mpv == "cb1" && Path::new(CB1_MPV).exists() {
            CB1_MPV
        } else {
            "mpv"
        };
        Mpv {
            socket: runtime.join("htpc-control-mpv.sock"),
            program: program.into(),
            config_dir: config.home.join(".config/jellyfin-mpv-shim"),
        }
    }

    async fn command(&self, command: Value) -> Result<Value> {
        tokio::time::timeout(TIMEOUT, self.command_inner(command))
            .await
            .map_err(|_| ApiError::backend("mpv did not answer in time."))?
    }

    async fn command_inner(&self, command: Value) -> Result<Value> {
        let stream = UnixStream::connect(&self.socket)
            .await
            .map_err(|_| ApiError::nothing_plays())?;
        let (reader, mut writer) = stream.into_split();
        let request = json!({"command": command, "request_id": 1});
        writer.write_all(format!("{request}\n").as_bytes()).await?;
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await? {
            let Ok(answer) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if answer["request_id"] == 1 {
                if answer["error"] != "success" {
                    return Err(ApiError::backend(format!(
                        "mpv refused the command: {}",
                        answer["error"]
                    )));
                }
                return Ok(answer["data"].clone());
            }
        }
        Err(ApiError::backend("mpv closed the connection."))
    }

    async fn property(&self, name: &str) -> Option<Value> {
        self.command(json!(["get_property", name])).await.ok()
    }

    pub async fn running(&self) -> bool {
        self.socket.exists() && self.command(json!(["get_property", "pid"])).await.is_ok()
    }

    pub async fn play(&self, url: &str, title: Option<&str>) -> Result<()> {
        if self.running().await {
            self.command(json!(["loadfile", url, "replace"])).await?;
            if let Some(title) = title {
                self.command(json!(["set_property", "force-media-title", title]))
                    .await?;
            }
            return Ok(());
        }
        let mut argv = vec![
            self.program.clone(),
            format!("--config-dir={}", self.config_dir.display()),
            format!("--input-ipc-server={}", self.socket.display()),
            "--fullscreen".into(),
        ];
        if let Some(title) = title {
            argv.push(format!("--force-media-title={title}"));
        }
        argv.extend(["--".into(), url.into()]);
        let unit = run_detached("htpc-control-mpv", &argv).await?;
        if self.program == CB1_MPV {
            tokio::spawn(give_screen_back_after(unit));
        }
        Ok(())
    }

    pub async fn status(&self) -> Option<Status> {
        if !self.socket.exists() {
            return None;
        }
        if self.property("idle-active").await? == json!(true) {
            return None;
        }
        let paused = self.property("pause").await == Some(json!(true));
        let buffering = self.property("paused-for-cache").await == Some(json!(true));
        Some(Status {
            source: Source::Url,
            state: if buffering {
                PlayState::Buffering
            } else if paused {
                PlayState::Paused
            } else {
                PlayState::Playing
            },
            title: self
                .property("media-title")
                .await
                .and_then(|t| t.as_str().map(String::from)),
            subtitle: None,
            position: self.property("time-pos").await.and_then(|t| t.as_f64()),
            duration: self.property("duration").await.and_then(|t| t.as_f64()),
            media_id: None,
            url: self
                .property("path")
                .await
                .and_then(|t| t.as_str().map(String::from)),
            image: None,
            running: Running::default(),
        })
    }

    pub async fn set_paused(&self, paused: Option<bool>) -> Result<()> {
        match paused {
            Some(paused) => self.command(json!(["set_property", "pause", paused])).await,
            None => self.command(json!(["cycle", "pause"])).await,
        }
        .map(|_| ())
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        self.command(json!(["seek", position, "absolute"]))
            .await
            .map(|_| ())
    }

    pub async fn quit(&self) -> Result<()> {
        self.command(json!(["quit"])).await.map(|_| ())
    }
}

/// The script htpc-screen.run of the CB1 mpv switches back to Plasma when
/// mpv ends. If mpv ends within a second of its start, for example at an
/// address that it cannot open, the script does not get to run, and the
/// screen stays on the empty VT of mpv. So after the end of the mpv of this
/// service, and when no other CB1 mpv runs, the service switches back.
async fn give_screen_back_after(unit: String) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let active = Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", &unit])
            .status()
            .await;
        if !matches!(active, Ok(status) if status.success()) {
            break;
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let front = std::fs::read_to_string("/sys/class/tty/tty0/active").unwrap_or_default();
    let other_mpv = Command::new("pgrep")
        .args(["-u", &uid(), "-f", CB1_MPV])
        .status()
        .await
        .is_ok_and(|s| s.success());
    if front.trim() != CB1_VT || other_mpv {
        return;
    }
    let Some(vt) = plasma_vt().await else { return };
    tracing::info!("mpv ended on its VT, switching back to VT {vt}");
    let _ = Command::new("busctl")
        .args([
            "call",
            "org.freedesktop.login1",
            "/org/freedesktop/login1/seat/seat0",
            "org.freedesktop.login1.Seat",
            "SwitchTo",
            "u",
            &vt,
        ])
        .status()
        .await;
}

/// The user ID of this process, from the owner of /proc/self
fn uid() -> String {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self")
        .map(|m| m.uid().to_string())
        .unwrap_or_default()
}

/// The VT of the graphical session of this user
async fn plasma_vt() -> Option<String> {
    let value = |args: Vec<String>| async move {
        let output = Command::new("loginctl")
            .args(args)
            .arg("--value")
            .output()
            .await
            .ok()?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string()).filter(|v| !v.is_empty())
    };
    let session = value(vec![
        "show-user".into(),
        uid(),
        "-p".into(),
        "Display".into(),
    ])
    .await?;
    value(vec![
        "show-session".into(),
        session,
        "-p".into(),
        "VTNr".into(),
    ])
    .await
    .filter(|vt| vt.chars().all(|c| c.is_ascii_digit()))
}
