// SPDX-License-Identifier: AGPL-3.0-only
//! Plays a media address that no app of the HTPC knows, for example a
//! file of a DLNA server, in mpv.
//!
//! mpv starts with the configuration folder of jellyfin-mpv-shim. So it
//! uses the same mpv program, options and scripts as the shim: the CB1 mpv
//! on its own VT with JELLYFIN_MPV=cb1, the refresh rate script, and the
//! TV wake. This service controls it over the JSON IPC of mpv.
//!
//! An audio address plays in a second kind of mpv, without a picture and
//! without the configuration. The CB1 mpv would switch the TV to its own
//! VT and show a black screen for each song.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::apps::run_detached;
use crate::config::{Config, MpvBuild};
use crate::error::{Error, Result};
use crate::status::{PlayState, Source, Status};

const CB1_MPV: &str = "/opt/x86-linux-htpc/cb1/bin/mpv";
const TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_ID: i64 = 1;
/// How long a start waits for an mpv of the other kind to end
#[cfg(not(test))]
const HANDOVER_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(test)]
const HANDOVER_TIMEOUT: Duration = Duration::from_millis(300);
/// File extensions of audio files. A DLNA control point also says it with
/// the class of the item, but a plain address has only the extension.
const AUDIO_EXTENSIONS: [&str; 11] = [
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "wma", "aiff", "alac",
];

/// What an address is: a video with its picture, or audio without one
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
}

impl MediaKind {
    /// The kind of the address. `audio_hint` is true when the sender says
    /// that the address is audio, for example by the class of a DLNA item.
    /// Otherwise the extension of the file decides.
    pub fn of(url: &Url, audio_hint: bool) -> MediaKind {
        if audio_hint || is_audio_address(url) {
            MediaKind::Audio
        } else {
            MediaKind::Video
        }
    }
}

/// Whether the path of the address ends in the extension of an audio file
fn is_audio_address(url: &Url) -> bool {
    url.path()
        .rsplit_once('.')
        .is_some_and(|(_, extension)| AUDIO_EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}

/// Starts a program with these arguments, detached from this service. The
/// tests replace it, so they start no program.
pub type Spawner =
    Arc<dyn Fn(Vec<String>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

fn systemd_spawner() -> Spawner {
    Arc::new(|argv| Box::pin(async move { run_detached("htpc-control-mpv", &argv).await }))
}

pub struct Mpv {
    socket: PathBuf,
    program: String,
    config_dir: PathBuf,
    spawner: Spawner,
}

impl Mpv {
    pub fn new(config: &Config) -> Mpv {
        let program = if config.jellyfin_mpv == MpvBuild::Cb1 && Path::new(CB1_MPV).exists() {
            CB1_MPV
        } else {
            "mpv"
        };
        Mpv {
            socket: config.runtime_dir.join("htpc-control-mpv.sock"),
            program: program.into(),
            config_dir: config.home.join(".config/jellyfin-mpv-shim"),
            spawner: systemd_spawner(),
        }
    }

    /// An mpv that starts its programs with the spawner. The tests use it.
    #[cfg(test)]
    pub fn with_spawner(config: &Config, spawner: Spawner) -> Mpv {
        Mpv {
            spawner,
            ..Mpv::new(config)
        }
    }

    async fn command(&self, command: Value) -> Result<Value> {
        tokio::time::timeout(TIMEOUT, self.command_without_timeout(command))
            .await
            .map_err(|_| Error::player("mpv did not answer in time."))?
    }

    /// One connection for each command. mpv also sends events on the
    /// connection, so the reader skips all lines without the request ID.
    async fn command_without_timeout(&self, command: Value) -> Result<Value> {
        let stream = UnixStream::connect(&self.socket)
            .await
            .map_err(|_| Error::nothing_plays())?;
        let (reader, mut writer) = stream.into_split();
        let request = json!({"command": command, "request_id": REQUEST_ID});
        writer.write_all(format!("{request}\n").as_bytes()).await?;
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await? {
            let Ok(answer) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if answer["request_id"] != REQUEST_ID {
                continue;
            }
            if answer["error"] != "success" {
                return Err(Error::player(format!(
                    "mpv refused the command: {}",
                    answer["error"]
                )));
            }
            return Ok(answer["data"].clone());
        }
        Err(Error::player("mpv closed the connection."))
    }

    async fn property(&self, name: &str) -> Option<Value> {
        self.command(json!(["get_property", name])).await.ok()
    }

    async fn text_property(&self, name: &str) -> Option<String> {
        self.property(name)
            .await
            .and_then(|value| value.as_str().map(String::from))
    }

    async fn number_property(&self, name: &str) -> Option<f64> {
        self.property(name).await.and_then(|value| value.as_f64())
    }

    async fn flag_property(&self, name: &str) -> bool {
        self.property(name).await == Some(json!(true))
    }

    async fn set_property(&self, name: &str, value: impl Into<Value>) -> Result<()> {
        self.command(json!(["set_property", name, value.into()]))
            .await?;
        Ok(())
    }

    pub async fn running(&self) -> bool {
        self.socket.exists() && self.property("pid").await.is_some()
    }

    /// Whether the mpv that runs plays audio only
    async fn plays_audio_only(&self) -> bool {
        self.text_property("options/vid").await.as_deref() == Some("no")
    }

    /// Ends the mpv that runs, and waits until its socket answers no more.
    async fn end_running(&self) -> Result<()> {
        self.quit().await?;
        let deadline = tokio::time::Instant::now() + HANDOVER_TIMEOUT;
        while self.running().await && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    /// Loads the address in the mpv that runs, or starts mpv for it. An mpv
    /// of the other kind, with or without picture, ends first.
    pub async fn play(&self, url: &str, title: Option<&str>, kind: MediaKind) -> Result<()> {
        if self.running().await && self.plays_audio_only().await != (kind == MediaKind::Audio) {
            self.end_running().await?;
        }
        if self.running().await {
            // An empty title gives the title back to mpv, so the title of
            // the last address does not stay.
            self.set_property("force-media-title", title.unwrap_or_default())
                .await?;
            self.command(json!(["loadfile", url, "replace"])).await?;
            return Ok(());
        }
        (self.spawner)(self.start_arguments(url, title, kind)).await
    }

    fn start_arguments(&self, url: &str, title: Option<&str>, kind: MediaKind) -> Vec<String> {
        let mut argv = if kind == MediaKind::Audio {
            // The mpv of the distribution without the configuration of the
            // shim: no VT switch, no TV wake, and the default PipeWire sink.
            vec![
                "mpv".to_string(),
                "--no-config".into(),
                "--no-video".into(),
                "--audio-display=no".into(),
            ]
        } else {
            vec![
                self.program.clone(),
                format!("--config-dir={}", self.config_dir.display()),
                "--fullscreen".into(),
            ]
        };
        argv.push(format!("--input-ipc-server={}", self.socket.display()));
        if let Some(title) = title {
            argv.push(format!("--force-media-title={title}"));
        }
        argv.extend(["--".into(), url.into()]);
        argv
    }

    /// The media that mpv plays, or None when mpv does not run or waits
    /// without a file
    pub async fn status(&self) -> Option<Status> {
        if !self.socket.exists() || self.property("idle-active").await? == json!(true) {
            return None;
        }
        let state = if self.flag_property("paused-for-cache").await {
            PlayState::Buffering
        } else if self.flag_property("pause").await {
            PlayState::Paused
        } else {
            PlayState::Playing
        };
        Some(Status {
            source: Source::Url,
            state,
            title: self.text_property("media-title").await,
            position: self.number_property("time-pos").await,
            duration: self.number_property("duration").await,
            url: self.text_property("path").await,
            ..Status::default()
        })
    }

    pub async fn set_paused(&self, paused: bool) -> Result<()> {
        self.set_property("pause", paused).await
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        self.command(json!(["seek", position, "absolute"])).await?;
        Ok(())
    }

    pub async fn quit(&self) -> Result<()> {
        self.command(json!(["quit"])).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    /// An mpv that answers each command with success, and records the
    /// commands
    fn fake_mpv(socket: PathBuf) -> tokio::sync::mpsc::UnboundedReceiver<Value> {
        fake_mpv_with_video(socket, None)
    }

    /// The same, for an mpv with the option `vid`. "no" is an mpv without
    /// picture.
    fn fake_mpv_with_video(
        socket: PathBuf,
        vid: Option<&'static str>,
    ) -> tokio::sync::mpsc::UnboundedReceiver<Value> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let listener = UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();
                let Ok(Some(line)) = lines.next_line().await else {
                    continue;
                };
                let request: Value = serde_json::from_str(&line).unwrap();
                let command = request["command"].clone();
                // The record comes first, so the test sees it once the
                // client has the answer.
                let _ = sender.send(command.clone());
                // An event before the answer, as mpv sends them
                let data = match (command[1].as_str(), vid) {
                    (Some("idle-active"), _) => json!(false),
                    (Some("options/vid"), Some(vid)) => json!(vid),
                    _ => json!(1),
                };
                let answer =
                    json!({"request_id": request["request_id"], "error": "success", "data": data});
                let _ = writer
                    .write_all(format!("{{\"event\":\"idle\"}}\n{answer}\n").as_bytes())
                    .await;
            }
        });
        receiver
    }

    fn test_mpv(name: &str) -> Mpv {
        let dir =
            std::env::temp_dir().join(format!("htpc-control-mpv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Mpv {
            socket: dir.join("mpv.sock"),
            program: "mpv".into(),
            config_dir: dir,
            spawner: Arc::new(|_| Box::pin(async { Ok(()) })),
        }
    }

    async fn commands_of_play(name: &str, title: Option<&str>) -> Vec<Value> {
        let mpv = test_mpv(name);
        let mut commands = fake_mpv(mpv.socket.clone());
        mpv.play("http://h/b.mkv", title, MediaKind::Video)
            .await
            .unwrap();
        let mut sent = Vec::new();
        while let Ok(command) = commands.try_recv() {
            sent.push(command);
        }
        let _ = std::fs::remove_dir_all(&mpv.config_dir);
        sent
    }

    #[tokio::test]
    async fn sets_the_title_before_the_address() {
        let sent = commands_of_play("title", Some("Film")).await;
        assert_eq!(
            sent,
            vec![
                json!(["get_property", "pid"]),
                json!(["get_property", "options/vid"]),
                json!(["get_property", "pid"]),
                json!(["set_property", "force-media-title", "Film"]),
                json!(["loadfile", "http://h/b.mkv", "replace"]),
            ]
        );
    }

    #[tokio::test]
    async fn clears_the_title_of_the_last_address() {
        let sent = commands_of_play("no-title", None).await;
        assert_eq!(
            sent.last().unwrap(),
            &json!(["loadfile", "http://h/b.mkv", "replace"])
        );
        assert_eq!(
            sent[sent.len() - 2],
            json!(["set_property", "force-media-title", ""])
        );
    }

    /// The commands that play sends to a running mpv with this option `vid`
    async fn commands_of_play_while_running(
        name: &str,
        vid: &'static str,
        url: &str,
        kind: MediaKind,
    ) -> Vec<Value> {
        let mpv = test_mpv(name);
        let mut commands = fake_mpv_with_video(mpv.socket.clone(), Some(vid));
        // The fake mpv keeps answering after "quit", so the start gives up
        // waiting for the end after a short time.
        let _ = mpv.play(url, None, kind).await;
        let mut sent = Vec::new();
        while let Ok(command) = commands.try_recv() {
            sent.push(command);
        }
        let _ = std::fs::remove_dir_all(&mpv.config_dir);
        sent
    }

    #[tokio::test]
    async fn an_audio_address_goes_to_the_running_audio_mpv() {
        let sent =
            commands_of_play_while_running("audio-audio", "no", "http://h/b.mp3", MediaKind::Audio)
                .await;
        assert!(sent.contains(&json!(["loadfile", "http://h/b.mp3", "replace"])));
        assert!(!sent.contains(&json!(["quit"])));
    }

    #[tokio::test]
    async fn a_video_address_ends_the_running_audio_mpv() {
        let sent =
            commands_of_play_while_running("audio-video", "no", "http://h/b.mkv", MediaKind::Video)
                .await;
        assert!(sent.contains(&json!(["quit"])));
    }

    #[tokio::test]
    async fn an_audio_address_ends_the_running_video_mpv() {
        let sent = commands_of_play_while_running(
            "video-audio",
            "auto",
            "http://h/b.mp3",
            MediaKind::Audio,
        )
        .await;
        assert!(sent.contains(&json!(["quit"])));
    }

    #[tokio::test]
    async fn a_video_address_goes_to_the_running_video_mpv() {
        let sent = commands_of_play_while_running(
            "video-video",
            "auto",
            "http://h/b.mkv",
            MediaKind::Video,
        )
        .await;
        assert!(sent.contains(&json!(["loadfile", "http://h/b.mkv", "replace"])));
        assert!(!sent.contains(&json!(["quit"])));
    }

    #[test]
    fn knows_the_kind_of_an_address() {
        let kind = |address: &str, hint: bool| MediaKind::of(&Url::parse(address).unwrap(), hint);
        assert_eq!(kind("http://h/music/Song.MP3", false), MediaKind::Audio);
        assert_eq!(kind("http://h/a.flac?token=1", false), MediaKind::Audio);
        assert_eq!(kind("http://h/film.mkv", false), MediaKind::Video);
        assert_eq!(kind("http://h/stream", false), MediaKind::Video);
        assert_eq!(kind("http://h/dir.mp3/video", false), MediaKind::Video);
        // The sender knows better than the extension.
        assert_eq!(kind("http://h/stream", true), MediaKind::Audio);
        assert_eq!(kind("http://h/film.mkv", true), MediaKind::Audio);
    }

    #[test]
    fn audio_starts_without_picture_and_configuration() {
        let mpv = test_mpv("audio-args");
        let argv = mpv.start_arguments("http://h/a.mp3", Some("Song"), MediaKind::Audio);
        assert_eq!(argv[0], "mpv");
        assert!(argv.contains(&"--no-config".to_string()));
        assert!(argv.contains(&"--no-video".to_string()));
        assert!(!argv.iter().any(|a| a.starts_with("--config-dir")));
        assert!(!argv.contains(&"--fullscreen".to_string()));
        assert_eq!(argv[argv.len() - 2..], ["--", "http://h/a.mp3"]);
        let video = mpv.start_arguments("http://h/a.mkv", None, MediaKind::Video);
        assert!(video.iter().any(|a| a.starts_with("--config-dir")));
        assert!(video.contains(&"--fullscreen".to_string()));
        let _ = std::fs::remove_dir_all(&mpv.config_dir);
    }

    #[tokio::test]
    async fn reports_no_media_without_mpv() {
        let mpv = test_mpv("none");
        assert!(!mpv.running().await);
        assert!(mpv.status().await.is_none());
        assert_eq!(
            mpv.quit().await.unwrap_err().kind,
            crate::error::ErrorKind::NothingPlays
        );
        let _ = std::fs::remove_dir_all(&mpv.config_dir);
    }
}
