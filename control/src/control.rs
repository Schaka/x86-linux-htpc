// SPDX-License-Identifier: AGPL-3.0-only
//! Starts media in the right player and sends the play keys to the player
//! that plays. The REST API and the DLNA renderer both use this.
//!
//! One player shows media at a time. Kodi and the CB1 mpv each take the
//! whole screen on their own VT, so a new start first stops the media of
//! the other players and ends Kodi.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify, watch};
use utoipa::ToSchema;

use crate::apps::{self, App, KODI_ENTRY, YOUTUBE_ENTRY};
use crate::config::Config;
use crate::desktop;
use crate::error::{Error, Result};
use crate::players::jellyfin::{self, Jellyfin, PlayCommand};
use crate::players::kodi;
use crate::players::mpv::{MediaKind, Mpv};
use crate::players::youtube::{self, Youtube};
use crate::status::{PlayState, Running, Source, Status};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const CONDITION_CHECK_INTERVAL: Duration = Duration::from_millis(500);
const WAKE: &str = "/usr/local/libexec/x86-linux-htpc/wake-devices";
const VACUUMTUBE: &str = "rocks.shy.VacuumTube";
const URL_SCHEMES: [&str; 6] = ["http", "https", "rtsp", "rtmp", "smb", "nfs"];
/// Kodi needs this long to start and to load its add-ons.
const KODI_START_TIMEOUT: Duration = Duration::from_secs(40);
/// The JSON-RPC interface of Kodi starts before the add-ons.
const KODI_ADDON_START: Duration = Duration::from_secs(3);
const KODI_EXIT_TIMEOUT: Duration = Duration::from_secs(15);
/// After Kodi ends, its launcher switches back to Plasma and shows the
/// home screen. An app that starts before that is hidden by the home screen.
const KODI_HANDOVER: Duration = Duration::from_secs(4);
/// The CB1 mpv gives the screen back to Plasma one second after a stop.
const SCREEN_HANDOVER: Duration = Duration::from_secs(2);

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

impl JellyfinPlayer {
    pub fn source(self) -> Source {
        match self {
            JellyfinPlayer::Shim => Source::Jellyfin,
            JellyfinPlayer::Kodi => Source::Kodi,
        }
    }
}

pub struct Controller {
    pub config: Config,
    jellyfin: Jellyfin,
    youtube: Youtube,
    mpv: Mpv,
    status: watch::Sender<Status>,
    refresh: Notify,
    /// One start at a time, so two starts do not stop each other.
    starting: Mutex<()>,
}

impl Controller {
    pub fn new(config: Config) -> Arc<Controller> {
        let mpv = Mpv::new(&config);
        Controller::with_mpv(config, mpv)
    }

    /// A controller with this mpv. The tests use a mpv that starts no program.
    pub fn with_mpv(config: Config, mpv: Mpv) -> Arc<Controller> {
        let http = reqwest::Client::new();
        Arc::new(Controller {
            jellyfin: Jellyfin::new(http.clone(), &config.home),
            youtube: Youtube::new(http),
            mpv,
            config,
            status: watch::channel(Status::default()).0,
            refresh: Notify::new(),
            starting: Mutex::new(()),
        })
    }

    /// The state of the last poll
    pub fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    /// Sets the state, as the poll does. The tests use it.
    #[cfg(test)]
    pub fn publish_status(&self, status: Status) {
        self.status.send_replace(status);
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

    /// Reads the players every POLL_INTERVAL, and at once after a command.
    pub async fn poll_forever(self: Arc<Self>) {
        loop {
            self.refresh_now().await;
            tokio::select! {
                () = tokio::time::sleep(POLL_INTERVAL) => {}
                () = self.refresh.notified() => {}
            }
        }
    }

    /// The first player in this order that has media: Kodi and the mpv of
    /// this service take the whole screen, so they are in front of the
    /// shim and VacuumTube.
    async fn read_players(&self) -> Status {
        let running = Running {
            youtube: youtube::running().await,
            kodi: kodi::running().await,
        };
        let mut found = None;
        if running.kodi {
            found = kodi::status().await.unwrap_or_else(log_no_status);
        }
        if found.is_none() {
            found = self.mpv.status().await;
        }
        if found.is_none() {
            found = self.jellyfin.status().await.unwrap_or_else(log_no_status);
        }
        if found.is_none() && running.youtube {
            found = self.youtube.status().await.unwrap_or_else(log_no_status);
        }
        Status {
            running,
            ..found.unwrap_or_default()
        }
    }

    /// Makes the poll read the players at once.
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
                self.jellyfin.command(PlayCommand::Stop).await?;
                tokio::time::sleep(SCREEN_HANDOVER).await;
            }
            Source::Youtube if keep != Source::Youtube => self.youtube.pause().await?,
            _ => {}
        }
        Ok(())
    }

    /// Starts VacuumTube without a video, for a launch request of DIAL.
    /// The other players make room first, as for a video.
    pub async fn start_youtube_app(&self) -> Result<()> {
        let _guard = self.starting.lock().await;
        self.make_room(Source::Youtube).await?;
        apps::launch(&self.config, YOUTUBE_ENTRY, &[]).await?;
        self.changed();
        Ok(())
    }

    pub async fn play_youtube(&self, video_id: &str, start: Option<f64>) -> Result<()> {
        if !youtube::valid_video_id(video_id) {
            return Err(Error::bad_request(format!(
                "{video_id} is not a YouTube video ID."
            )));
        }
        let _guard = self.starting.lock().await;
        self.make_room(Source::Youtube).await?;
        let address = youtube::watch_url(video_id, start);
        apps::launch(&self.config, YOUTUBE_ENTRY, &[address]).await?;
        self.changed();
        Ok(())
    }

    /// Plays a Jellyfin item. Without `start`, `resume` starts at the
    /// resume position of the Jellyfin user of the player.
    pub async fn play_jellyfin(
        &self,
        item_id: &str,
        start: Option<f64>,
        resume: bool,
        player: JellyfinPlayer,
    ) -> Result<()> {
        if !jellyfin::valid_item_id(item_id) {
            return Err(Error::bad_request(format!(
                "{item_id} is not a Jellyfin item ID."
            )));
        }
        let _guard = self.starting.lock().await;
        let start = match (start, resume, player) {
            (Some(start), _, _) => Some(start),
            (None, false, _) => None,
            (None, true, JellyfinPlayer::Shim) => self.jellyfin.resume_position(item_id).await?,
            (None, true, JellyfinPlayer::Kodi) => {
                self.jellyfin.kodi_resume_position(item_id).await?
            }
        };
        self.make_room(player.source()).await?;
        match player {
            JellyfinPlayer::Shim => self.jellyfin.play(item_id, start).await?,
            JellyfinPlayer::Kodi => {
                self.start_kodi().await?;
                kodi::open(&kodi::jellyfin_addon_file(item_id, start)).await?;
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
            return Err(Error::player("Kodi did not start in time."));
        }
        tokio::time::sleep(KODI_ADDON_START).await;
        Ok(())
    }

    /// Plays a media address in the player that fits it: a YouTube address
    /// in VacuumTube, a stream of the Jellyfin server in the shim, and all
    /// other addresses in mpv. `audio` says that the address is audio, for
    /// example by the class of a DLNA item. The extension of the address
    /// also decides it.
    pub async fn play_url(
        &self,
        address: &str,
        title: Option<&str>,
        audio: bool,
    ) -> Result<Source> {
        let url = Url::parse(address)
            .map_err(|_| Error::bad_request(format!("{address} is not an address.")))?;
        if !URL_SCHEMES.contains(&url.scheme()) {
            return Err(Error::bad_request(format!(
                "The scheme {} is not allowed.",
                url.scheme()
            )));
        }
        if let Some(video_id) = youtube::video_id_from_url(&url) {
            self.play_youtube(&video_id, youtube::start_from_url(&url))
                .await?;
            return Ok(Source::Youtube);
        }
        if let Some(item_id) = self.jellyfin.item_id_from_url(&url) {
            self.play_jellyfin(&item_id, None, false, JellyfinPlayer::Shim)
                .await?;
            return Ok(Source::Jellyfin);
        }
        let _guard = self.starting.lock().await;
        self.make_room(Source::Url).await?;
        self.mpv
            .play(address, title, MediaKind::of(&url, audio))
            .await?;
        self.changed();
        Ok(Source::Url)
    }

    pub async fn launch_app(&self, id: &str) -> Result<App> {
        let app = apps::find(&self.config, id)
            .ok_or_else(|| Error::not_found(format!("No app has the name {id}.")))?;
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

    /// Some(true) pauses, Some(false) plays, None toggles.
    pub async fn set_paused(&self, paused: Option<bool>) -> Result<()> {
        let status = self.refresh_now().await;
        let paused = paused.unwrap_or(status.state != PlayState::Paused);
        match status.source {
            Source::Idle => return Err(Error::nothing_plays()),
            Source::Jellyfin => self.jellyfin.set_paused(paused).await?,
            Source::Youtube => self.youtube.set_paused(paused).await?,
            Source::Kodi => kodi::set_paused(paused).await?,
            Source::Url => self.mpv.set_paused(paused).await?,
        }
        self.changed();
        Ok(())
    }

    /// Stops the media. Kodi and VacuumTube keep running.
    pub async fn stop(&self) -> Result<()> {
        match self.refresh_now().await.source {
            Source::Idle => return Err(Error::nothing_plays()),
            Source::Jellyfin => self.jellyfin.command(PlayCommand::Stop).await?,
            Source::Youtube => self.youtube.stop().await?,
            Source::Kodi => kodi::stop().await?,
            Source::Url => self.mpv.quit().await?,
        }
        self.changed();
        Ok(())
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        if !position.is_finite() || position < 0.0 {
            return Err(Error::bad_request(
                "The position must be 0 or more seconds.",
            ));
        }
        match self.refresh_now().await.source {
            Source::Idle => return Err(Error::nothing_plays()),
            Source::Jellyfin => self.jellyfin.seek(position).await?,
            Source::Youtube => self.youtube.seek(position).await?,
            Source::Kodi => kodi::seek(position).await?,
            Source::Url => self.mpv.seek(position).await?,
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
            desktop::kill_flatpak(VACUUMTUBE).await;
        }
        desktop::minimize_windows(&self.config.runtime_dir).await;
        self.changed();
        Ok(())
    }

    pub async fn wake_tv(&self) -> Result<()> {
        if !Path::new(WAKE).exists() {
            return Err(Error::not_installed(
                "The component network-wake is not installed.",
            ));
        }
        let status = tokio::process::Command::new(WAKE).status().await?;
        if !status.success() {
            return Err(Error::player(
                "The Wake-on-LAN sender failed. Read WAKE_ON_LAN_MACS in htpc.conf.",
            ));
        }
        Ok(())
    }
}

/// A player that does not answer counts as a player without media.
fn log_no_status<T>(error: Error) -> Option<T> {
    tracing::debug!("player state: {error}");
    None
}

/// Checks the condition until it is true or the time is over. Returns
/// false when the time is over.
async fn wait_until<F, Fut>(timeout: Duration, mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if condition().await {
            return true;
        }
        tokio::time::sleep(CONDITION_CHECK_INTERVAL).await;
    }
    false
}
