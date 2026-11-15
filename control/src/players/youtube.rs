// SPDX-License-Identifier: AGPL-3.0-only
//! Controls the YouTube player in VacuumTube.
//!
//! The component youtube-tv turns on the debugging port of Chromium in
//! VacuumTube, on localhost only. Its launcher reads the player there for
//! the refresh rate, and this service uses the same port for the state and
//! the play keys. The player object of the YouTube TV page has the methods
//! of the YouTube IFrame player, for example getPlayerState and seekTo.

use std::fmt::Display;
use std::sync::Mutex;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

use crate::error::{Error, Result};
use crate::status::{PlayState, Source, Status};

pub const DEBUG_PORT: u16 = 47811;
const TIMEOUT: Duration = Duration::from_secs(3);
const OEMBED_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_ID: i64 = 1;

const PLAYER_JS: &str = r#"[document.getElementById("movie_player"),
  document.querySelector(".html5-video-player")].find(e => e && e.getPlayerState)"#;

/// The title and the channel of a video
#[derive(Clone, Debug, Default)]
struct VideoNames {
    title: Option<String>,
    author: Option<String>,
}

pub struct Youtube {
    http: reqwest::Client,
    /// The names of the last video, because the poll asks for them every
    /// few seconds
    names: Mutex<Option<(String, VideoNames)>>,
}

/// True while VacuumTube runs, because only VacuumTube opens the port.
pub async fn running() -> bool {
    super::port_open(DEBUG_PORT).await
}

impl Youtube {
    pub fn new(http: reqwest::Client) -> Youtube {
        Youtube {
            http,
            names: Mutex::new(None),
        }
    }

    /// Runs one JavaScript expression in the page and returns its value.
    async fn evaluate(&self, expression: &str) -> Result<Value> {
        tokio::time::timeout(TIMEOUT, self.evaluate_without_timeout(expression))
            .await
            .map_err(|_| Error::player("VacuumTube did not answer in time."))?
    }

    async fn evaluate_without_timeout(&self, expression: &str) -> Result<Value> {
        let targets: Value = self
            .http
            .get(format!("http://127.0.0.1:{DEBUG_PORT}/json"))
            .send()
            .await
            .map_err(no_answer)?
            .json()
            .await
            .map_err(no_answer)?;
        let socket_url = targets
            .as_array()
            .and_then(|targets| targets.iter().find(|target| target["type"] == "page"))
            .and_then(|page| page["webSocketDebuggerUrl"].as_str())
            .ok_or_else(|| Error::offline("VacuumTube has no page."))?;
        let (mut socket, _) = tokio_tungstenite::connect_async(socket_url)
            .await
            .map_err(no_answer)?;
        let request = json!({"id": REQUEST_ID, "method": "Runtime.evaluate",
                             "params": {"expression": expression, "returnByValue": true}});
        socket
            .send(Message::text(request.to_string()))
            .await
            .map_err(no_answer)?;
        // Chromium also sends events on the socket. Only the answer with
        // the request ID counts.
        while let Some(message) = socket.next().await {
            let Ok(Message::Text(text)) = message else {
                continue;
            };
            let Ok(answer) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if answer["id"] == REQUEST_ID {
                let _ = socket.close(None).await;
                return Ok(answer["result"]["result"]["value"].clone());
            }
        }
        Err(Error::player("VacuumTube closed the connection."))
    }

    /// The video that plays, or None in the menus
    pub async fn status(&self) -> Result<Option<Status>> {
        let script = format!(
            r##"(() => {{
              const p = {PLAYER_JS};
              if (!p || !location.hash.startsWith("#/watch")) return null;
              const d = p.getVideoData ? p.getVideoData() : {{}};
              return {{state: p.getPlayerState(), time: p.getCurrentTime(), duration: p.getDuration(),
                       id: d.video_id, title: d.title, author: d.author}};
            }})()"##
        );
        let value = self.evaluate(&script).await?;
        if !value.is_object() {
            return Ok(None);
        }
        let Some(state) = value["state"].as_i64().and_then(play_state) else {
            return Ok(None);
        };
        let id = non_empty(&value["id"]);
        let mut names = VideoNames {
            title: non_empty(&value["title"]),
            author: non_empty(&value["author"]),
        };
        if names.title.is_none()
            && let Some(id) = &id
        {
            names = self.video_names(id).await;
        }
        Ok(Some(Status {
            source: Source::Youtube,
            state,
            title: names.title,
            subtitle: names.author,
            position: value["time"].as_f64(),
            duration: value["duration"].as_f64().filter(|d| *d > 0.0),
            image: id
                .as_ref()
                .map(|id| format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
            url: id.as_ref().map(|id| watch_url(id, None)),
            media_id: id,
            ..Status::default()
        }))
    }

    /// The TV interface of YouTube gives the player no title, so the names
    /// come from the public oEmbed address of YouTube, once for each video.
    async fn video_names(&self, video_id: &str) -> VideoNames {
        if let Some((id, names)) = self.names.lock().unwrap().as_ref()
            && id == video_id
        {
            return names.clone();
        }
        let names = self.fetch_video_names(video_id).await.unwrap_or_default();
        *self.names.lock().unwrap() = Some((video_id.to_string(), names.clone()));
        names
    }

    async fn fetch_video_names(&self, video_id: &str) -> Option<VideoNames> {
        let mut url = Url::parse("https://www.youtube.com/oembed").ok()?;
        url.query_pairs_mut()
            .append_pair("format", "json")
            .append_pair("url", &watch_url(video_id, None));
        let answer: Value = self
            .http
            .get(url)
            .timeout(OEMBED_TIMEOUT)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()?;
        Some(VideoNames {
            title: non_empty(&answer["title"]),
            author: non_empty(&answer["author_name"]),
        })
    }

    /// Runs a statement on the player object `p`.
    async fn player_call(&self, call: &str) -> Result<()> {
        let script = format!(
            "(() => {{ const p = {PLAYER_JS}; if (!p) return false; {call}; return true; }})()"
        );
        match self.evaluate(&script).await? {
            Value::Bool(true) => Ok(()),
            _ => Err(Error::nothing_plays()),
        }
    }

    pub async fn pause(&self) -> Result<()> {
        self.player_call("p.pauseVideo()").await
    }

    pub async fn resume(&self) -> Result<()> {
        self.player_call("p.playVideo()").await
    }

    pub async fn set_paused(&self, paused: bool) -> Result<()> {
        if paused {
            self.pause().await
        } else {
            self.resume().await
        }
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        self.player_call(&format!("p.seekTo({position}, true)"))
            .await
    }

    /// Ends the video and goes back to the start page of YouTube. VacuumTube
    /// keeps running, so the next video starts at once.
    pub async fn stop(&self) -> Result<()> {
        self.player_call(r##"p.stopVideo(); location.hash = "#/""##)
            .await
    }
}

fn no_answer(error: impl Display) -> Error {
    Error::offline(format!("VacuumTube does not answer: {error}"))
}

fn non_empty(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(String::from)
}

/// The states of the IFrame player: -1 unstarted, 0 ended, 1 playing,
/// 2 paused, 3 buffering, 5 cued. An ended video counts as no video.
fn play_state(state: i64) -> Option<PlayState> {
    match state {
        1 => Some(PlayState::Playing),
        2 => Some(PlayState::Paused),
        -1 | 3 | 5 => Some(PlayState::Buffering),
        _ => None,
    }
}

pub fn watch_url(video_id: &str, start: Option<f64>) -> String {
    match start {
        Some(start) if start >= 1.0 => {
            format!(
                "https://www.youtube.com/watch?v={video_id}&t={}s",
                start as u64
            )
        }
        _ => format!("https://www.youtube.com/watch?v={video_id}"),
    }
}

/// The video ID of a YouTube address: watch?v=ID, youtu.be/ID, /shorts/ID,
/// /live/ID or /embed/ID
pub fn video_id_from_url(url: &Url) -> Option<String> {
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .trim_start_matches("m.");
    let mut segments = url.path_segments()?;
    let id = match (host, segments.next()) {
        ("youtu.be", Some(id)) => id.to_string(),
        ("youtube.com" | "music.youtube.com" | "youtube-nocookie.com", Some("watch")) => url
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, id)| id.into_owned())?,
        (
            "youtube.com" | "music.youtube.com" | "youtube-nocookie.com",
            Some("shorts" | "live" | "embed" | "v"),
        ) => segments.next()?.to_string(),
        _ => return None,
    };
    valid_video_id(&id).then_some(id)
}

/// The start time of a YouTube address, for example t=90 or t=90s
pub fn start_from_url(url: &Url) -> Option<f64> {
    url.query_pairs()
        .find(|(key, _)| key == "t")
        .and_then(|(_, time)| time.trim_end_matches('s').parse().ok())
}

pub fn valid_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn id(text: &str) -> Option<String> {
        video_id_from_url(&url(text))
    }

    #[test]
    fn finds_video_ids() {
        assert_eq!(
            id("https://www.youtube.com/watch?v=H3Cnyj0I62c&t=5").as_deref(),
            Some("H3Cnyj0I62c")
        );
        assert_eq!(
            id("https://youtu.be/H3Cnyj0I62c?si=x").as_deref(),
            Some("H3Cnyj0I62c")
        );
        assert_eq!(
            id("https://m.youtube.com/shorts/H3Cnyj0I62c").as_deref(),
            Some("H3Cnyj0I62c")
        );
        assert_eq!(id("https://www.youtube.com/channel/UCabc"), None);
        assert_eq!(id("https://example.org/watch?v=H3Cnyj0I62c"), None);
    }

    #[test]
    fn reads_start_times() {
        assert_eq!(
            start_from_url(&url("https://youtu.be/H3Cnyj0I62c?t=90s")),
            Some(90.0)
        );
        assert_eq!(start_from_url(&url("https://youtu.be/H3Cnyj0I62c")), None);
    }

    #[test]
    fn writes_start_times() {
        assert_eq!(
            watch_url("H3Cnyj0I62c", Some(0.5)),
            watch_url("H3Cnyj0I62c", None)
        );
    }

    #[test]
    fn maps_player_states() {
        assert_eq!(play_state(1), Some(PlayState::Playing));
        assert_eq!(play_state(2), Some(PlayState::Paused));
        assert_eq!(play_state(-1), Some(PlayState::Buffering));
        assert_eq!(play_state(5), Some(PlayState::Buffering));
        assert_eq!(play_state(0), None);
        assert!(valid_video_id("H3Cnyj0I62c"));
        assert!(!valid_video_id("H3Cnyj0I62"));
        assert!(!valid_video_id("H3Cnyj0I6/c"));
    }

    #[test]
    fn writes_whole_seconds() {
        assert_eq!(
            watch_url("H3Cnyj0I62c", Some(90.4)),
            "https://www.youtube.com/watch?v=H3Cnyj0I62c&t=90s"
        );
        assert_eq!(
            watch_url("H3Cnyj0I62c", None),
            "https://www.youtube.com/watch?v=H3Cnyj0I62c"
        );
    }
}
