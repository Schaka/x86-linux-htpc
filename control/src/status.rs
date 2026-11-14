// SPDX-License-Identifier: AGPL-3.0-only
//! The state of the players, as the API and the DLNA renderer report it.

use serde::Serialize;
use utoipa::ToSchema;

/// The player that shows the current media
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Nothing plays
    Idle,
    /// jellyfin-mpv-shim
    Jellyfin,
    /// VacuumTube
    Youtube,
    /// Kodi
    Kodi,
    /// The mpv of this service, for a media address
    Url,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlayState {
    Idle,
    Playing,
    Paused,
    /// The player loads the media
    Buffering,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct Status {
    pub source: Source,
    pub state: PlayState,
    /// The title of the movie, episode or video
    pub title: Option<String>,
    /// The series with season and episode, or the YouTube channel
    pub subtitle: Option<String>,
    /// Seconds from the start
    pub position: Option<f64>,
    /// Seconds
    pub duration: Option<f64>,
    /// The Jellyfin item ID or the YouTube video ID
    pub media_id: Option<String>,
    /// The media address, for the source url
    pub url: Option<String>,
    /// An image of the media, for a media card
    pub image: Option<String>,
    /// The apps that run, also without media
    pub running: Running,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, ToSchema)]
pub struct Running {
    pub youtube: bool,
    pub kodi: bool,
}

impl Status {
    pub fn idle(running: Running) -> Status {
        Status {
            source: Source::Idle,
            state: PlayState::Idle,
            title: None,
            subtitle: None,
            position: None,
            duration: None,
            media_id: None,
            url: None,
            image: None,
            running,
        }
    }

    /// The same media and state, without the position. A DLNA event goes
    /// out only when this part changes.
    pub fn same_media_and_state(&self, other: &Status) -> bool {
        self.source == other.source
            && self.state == other.state
            && self.title == other.title
            && self.media_id == other.media_id
            && self.url == other.url
            && self.duration.map(|d| d.round()) == other.duration.map(|d| d.round())
    }
}
