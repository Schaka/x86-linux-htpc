// SPDX-License-Identifier: AGPL-3.0-only
//! Starts media in the right player and sends the play keys to the player
//! that plays. The REST API and the DLNA renderer both use this.
//!
//! One player shows media at a time. Kodi and the CB1 mpv each take the
//! whole screen on their own VT, so a new start first stops the media of
//! the other players and ends Kodi.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify, watch};
use utoipa::ToSchema;

use crate::apps::{self, App, KODI_ENTRY, YOUTUBE_ENTRY};
use crate::config::Config;
use crate::error::{ApiError, Result};
use crate::jellyfin::Jellyfin;
use crate::mpv::Mpv;
use crate::status::{PlayState, Running, Source, Status};
use crate::{kodi, youtube};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const WAKE: &str = "/usr/local/libexec/x86-linux-htpc/wake-devices";
const VACUUMTUBE: &str = "rocks.shy.VacuumTube";
/// Kodi needs this long to start and to load its add-ons.
const KODI_START_TIMEOUT: Duration = Duration::from_secs(40);
const KODI_EXIT_TIMEOUT: Duration = Duration::from_secs(15);
/// After Kodi ends, its launcher switches back to Plasma and shows the
/// home screen. An app that starts before that is hidden by the home screen.
const KODI_HANDOVER: Duration = Duration::from_secs(4);
/// The CB1 mpv gives the screen back to Plasma one second after a stop.
const SCREEN_HANDOVER: Duration = Duration::from_secs(2);
/// See unpause_after_wake
const WAKE_RESUME_WINDOW: Duration = Duration::from_secs(30);

/// The player of a Jellyfin item
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum JellyfinPlayer {
    /// jellyfin-mpv-shim
    #[default]
    Shim,
    /// Kodi with the add-on "Jellyfin for Kodi"
    Kodi,
}

pub struct Controller {
    pub config: Config,
    jellyfin: Arc<Jellyfin>,
    mpv: Mpv,
    status: watch::Sender<Status>,
    refresh: Notify,
    /// One start at a time, so two starts do not stop each other.
    starting: Mutex<()>,
}

impl Controller {
    pub fn new(config: Config) -> Arc<Controller> {
        let http = reqwest::Client::builder()
            .build()
            .expect("the HTTP client starts");
        Arc::new(Controller {
            jellyfin: Arc::new(Jellyfin::new(http, &config.home)),
            mpv: Mpv::new(&config),
            config,
            status: watch::channel(Status::idle(Running::default())).0,
            refresh: Notify::new(),
            starting: Mutex::new(()),
        })
    }

    pub fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Status> {
        self.status.subscribe()
    }

    /// Reads the players again now, for example right after a command.
    pub async fn refresh_now(&self) -> Status {
        let status = self.read_players().await;
        self.status.send_replace(status.clone());
        status
    }

    pub async fn poll_forever(self: Arc<Self>) {
        loop {
            self.refresh_now().await;
            tokio::select! {
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
                _ = self.refresh.notified() => {}
            }
        }
    }

    async fn read_players(&self) -> Status {
        let running = Running {
            youtube: youtube::running().await,
            kodi: kodi::running().await,
        };
        let mut found = None;
        if running.kodi {
            found = kodi::status().await.unwrap_or_else(log_none);
        }
        if found.is_none() {
            found = self.mpv.status().await;
        }
        if found.is_none() {
            found = self.jellyfin.status().await.unwrap_or_else(log_none);
        }
        if found.is_none() && running.youtube {
            found = youtube::status().await.unwrap_or_else(log_none);
        }
        let mut status = found.unwrap_or_else(|| Status::idle(running.clone()));
        status.running = running;
        status
    }

    fn changed(&self) {
        self.refresh.notify_one();
    }

    /// Stops the media of all players except `keep`, and ends Kodi unless
    /// Kodi plays next.
    async fn make_room(&self, keep: Source) -> Result<()> {
        let status = self.refresh_now().await;
        if keep != Source::Kodi && status.running.kodi {
            kodi::quit().await?;
            wait_until(KODI_EXIT_TIMEOUT, || async { !kodi::running().await }).await;
            tokio::time::sleep(KODI_HANDOVER).await;
        }
        if keep != Source::Url && self.mpv.running().await {
            self.mpv.quit().await?;
            tokio::time::sleep(SCREEN_HANDOVER).await;
        }
        match status.source {
            Source::Jellyfin if keep != Source::Jellyfin => {
                self.jellyfin.command("Stop").await?;
                tokio::time::sleep(SCREEN_HANDOVER).await;
            }
            Source::Youtube if keep != Source::Youtube => youtube::pause().await?,
            _ => {}
        }
        Ok(())
    }

    pub async fn play_youtube(&self, video_id: &str, start: Option<f64>) -> Result<()> {
        if !youtube::valid_video_id(video_id) {
            return Err(ApiError::bad_request(format!(
                "{video_id} is not a YouTube video ID."
            )));
        }
        let _guard = self.starting.lock().await;
        self.make_room(Source::Youtube).await?;
        apps::launch(
            &self.config,
            YOUTUBE_ENTRY,
            &[youtube::watch_url(video_id, start)],
        )
        .await?;
        self.changed();
        Ok(())
    }

    pub async fn play_jellyfin(
        &self,
        item_id: &str,
        start: Option<f64>,
        resume: bool,
        player: JellyfinPlayer,
    ) -> Result<()> {
        if item_id.is_empty() || !item_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(ApiError::bad_request(format!(
                "{item_id} is not a Jellyfin item ID."
            )));
        }
        let _guard = self.starting.lock().await;
        let start = match start {
            Some(start) => Some(start),
            None if resume => self.jellyfin.resume_position(item_id).await?,
            None => None,
        };
        match player {
            JellyfinPlayer::Shim => {
                self.make_room(Source::Jellyfin).await?;
                self.jellyfin.play(item_id, start).await?;
                tokio::spawn(unpause_after_wake(self.jellyfin.clone()));
            }
            JellyfinPlayer::Kodi => {
                self.make_room(Source::Kodi).await?;
                self.start_kodi().await?;
                // The add-on and the shim log in to the same Jellyfin server.
                // The add-on starts at the position of the parameter
                // timestamp, in seconds.
                let mut file = format!(
                    "plugin://plugin.video.jellyfin/?mode=play&id={}",
                    item_id.replace('-', "")
                );
                if let Some(start) = start.filter(|s| *s >= 1.0) {
                    file.push_str(&format!("&timestamp={}", start as u64));
                }
                kodi::open(&file).await?;
            }
        }
        self.changed();
        Ok(())
    }

    async fn start_kodi(&self) -> Result<()> {
        if kodi::running().await {
            return Ok(());
        }
        apps::launch(&self.config, KODI_ENTRY, &[]).await?;
        let ready = wait_until(KODI_START_TIMEOUT, || async {
            kodi::call("JSONRPC.Ping", serde_json::json!({}))
                .await
                .is_ok()
        })
        .await;
        if !ready {
            return Err(ApiError::backend("Kodi did not start in time."));
        }
        // The JSON-RPC interface starts before the add-ons.
        tokio::time::sleep(Duration::from_secs(3)).await;
        Ok(())
    }

    /// Plays a media address in the player that fits it: a YouTube address
    /// in VacuumTube, a stream of the Jellyfin server in the shim, and all
    /// other addresses in mpv.
    pub async fn play_url(&self, address: &str, title: Option<&str>) -> Result<Source> {
        let url = Url::parse(address)
            .map_err(|_| ApiError::bad_request(format!("{address} is not an address.")))?;
        if !matches!(
            url.scheme(),
            "http" | "https" | "rtsp" | "rtmp" | "smb" | "nfs"
        ) {
            return Err(ApiError::bad_request(format!(
                "The scheme {} is not allowed.",
                url.scheme()
            )));
        }
        if let Some(video_id) = youtube::video_id_from_url(&url) {
            let start = url
                .query_pairs()
                .find(|(k, _)| k == "t")
                .and_then(|(_, t)| t.trim_end_matches('s').parse().ok());
            self.play_youtube(&video_id, start).await?;
            return Ok(Source::Youtube);
        }
        if let Some(item_id) = self.jellyfin.item_id_from_url(&url) {
            self.play_jellyfin(&item_id, None, false, JellyfinPlayer::Shim)
                .await?;
            return Ok(Source::Jellyfin);
        }
        let _guard = self.starting.lock().await;
        self.make_room(Source::Url).await?;
        self.mpv.play(address, title).await?;
        self.changed();
        Ok(Source::Url)
    }

    pub async fn launch_app(&self, id: &str) -> Result<App> {
        let app = apps::find(&self.config, id)
            .ok_or_else(|| ApiError::not_found(format!("No app has the name {id}.")))?;
        let _guard = self.starting.lock().await;
        let keep = match app.desktop_id.as_str() {
            YOUTUBE_ENTRY => Source::Youtube,
            KODI_ENTRY => Source::Kodi,
            _ => Source::Idle,
        };
        self.make_room(keep).await?;
        apps::launch(&self.config, &app.desktop_id, &[]).await?;
        self.changed();
        Ok(app)
    }

    async fn playing_source(&self) -> Result<Source> {
        match self.refresh_now().await.source {
            Source::Idle => Err(ApiError::nothing_plays()),
            source => Ok(source),
        }
    }

    /// Some(true) pauses, Some(false) plays, None toggles.
    pub async fn set_paused(&self, paused: Option<bool>) -> Result<()> {
        let source = self.playing_source().await?;
        let state = self.status().state;
        let paused = paused.unwrap_or(state != PlayState::Paused);
        match source {
            Source::Jellyfin => {
                self.jellyfin
                    .command(if paused { "Pause" } else { "Unpause" })
                    .await?
            }
            Source::Youtube if paused => youtube::pause().await?,
            Source::Youtube => youtube::resume().await?,
            Source::Kodi => kodi::set_paused(Some(paused)).await?,
            Source::Url => self.mpv.set_paused(Some(paused)).await?,
            Source::Idle => unreachable!(),
        }
        self.changed();
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        match self.playing_source().await? {
            Source::Jellyfin => self.jellyfin.command("Stop").await?,
            Source::Youtube => youtube::stop().await?,
            Source::Kodi => kodi::stop().await?,
            Source::Url => self.mpv.quit().await?,
            Source::Idle => unreachable!(),
        }
        self.changed();
        Ok(())
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        if !position.is_finite() || position < 0.0 {
            return Err(ApiError::bad_request(
                "The position must be 0 or more seconds.",
            ));
        }
        match self.playing_source().await? {
            Source::Jellyfin => self.jellyfin.seek(position).await?,
            Source::Youtube => youtube::seek(position).await?,
            Source::Kodi => kodi::seek(position).await?,
            Source::Url => self.mpv.seek(position).await?,
            Source::Idle => unreachable!(),
        }
        self.changed();
        Ok(())
    }

    /// Stops all media, ends Kodi and VacuumTube, and minimizes the other
    /// windows, for example the one of Waydroid, so the home screen shows.
    /// The Android apps keep running in the background.
    pub async fn home(&self) -> Result<()> {
        let _guard = self.starting.lock().await;
        self.make_room(Source::Idle).await?;
        if youtube::running().await {
            let _ = tokio::process::Command::new("flatpak")
                .args(["kill", VACUUMTUBE])
                .status()
                .await;
        }
        minimize_windows().await;
        self.changed();
        Ok(())
    }

    pub async fn wake_tv(&self) -> Result<()> {
        if !std::path::Path::new(WAKE).exists() {
            return Err(ApiError::not_installed(
                "The component network-wake is not installed.",
            ));
        }
        let status = tokio::process::Command::new(WAKE).status().await?;
        if !status.success() {
            return Err(ApiError::backend(
                "The Wake-on-LAN sender failed. Read WAKE_ON_LAN_MACS in htpc.conf.",
            ));
        }
        Ok(())
    }
}

/// The KWin script of minimize_windows
const MINIMIZE_SCRIPT: &str = "for (const w of workspace.windowList()) {
    if (w.normalWindow && !w.minimized) w.minimized = true;
}
";

/// Minimizes all app windows through a KWin script. The home screen of
/// Plasma Bigscreen is the desktop, so it shows when no app window is open.
/// The shortcut of the Home key toggles the home screen, and Bigscreen
/// reports no state, so a toggle could hide the home screen instead.
async fn minimize_windows() {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = runtime.join("htpc-control-home.js");
    if std::fs::write(&path, MINIMIZE_SCRIPT).is_err() {
        return;
    }
    let busctl = |args: Vec<String>| async move {
        tokio::process::Command::new("busctl")
            .arg("--user")
            .args(args)
            .output()
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    };
    let name = "htpc-control-home".to_string();
    let scripting = |method: &str| {
        vec![
            "call".to_string(),
            "org.kde.KWin".into(),
            "/Scripting".into(),
            "org.kde.kwin.Scripting".into(),
            method.into(),
        ]
    };
    // A script of an earlier call with the same name blocks the load.
    let mut unload = scripting("unloadScript");
    unload.extend(["s".into(), name.clone()]);
    busctl(unload.clone()).await;
    let mut load = scripting("loadScript");
    load.extend(["ss".into(), path.display().to_string(), name]);
    let id = busctl(load)
        .await
        .and_then(|out| out.split_whitespace().nth(1).map(String::from))
        .filter(|id| id.chars().all(|c| c.is_ascii_digit()));
    if let Some(id) = id {
        busctl(vec![
            "call".into(),
            "org.kde.KWin".into(),
            format!("/Scripting/Script{id}"),
            "org.kde.kwin.Script".into(),
            "run".into(),
        ])
        .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    busctl(unload).await;
    let _ = std::fs::remove_file(path);
}

/// The start of a video wakes the TV (component network-wake). The AV
/// receiver then syncs the HDMI signal again, maybe more than once, and the
/// HDMI audio device goes away for a moment each time. mpv then pauses. So
/// for a short time after a start, the service continues a paused video. A
/// pause by the user in this time also ends.
async fn unpause_after_wake(jellyfin: Arc<Jellyfin>) {
    let deadline = tokio::time::Instant::now() + WAKE_RESUME_WINDOW;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Ok(Some(status)) = jellyfin.status().await
            && status.state == PlayState::Paused
        {
            tracing::info!("the video paused after the TV wake, continuing it");
            let _ = jellyfin.command("Unpause").await;
        }
    }
}

fn log_none<T>(error: ApiError) -> Option<T> {
    tracing::debug!("player state: {error}");
    None
}

async fn wait_until<F, Fut>(timeout: Duration, mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if condition().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}
