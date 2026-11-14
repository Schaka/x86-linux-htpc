// SPDX-License-Identifier: AGPL-3.0-only
//! Controls the YouTube player in VacuumTube.
//!
//! The component youtube-tv turns on the debugging port of Chromium in
//! VacuumTube, on localhost only. Its launcher reads the player there for
//! the refresh rate, and this service uses the same port for the state and
//! the play keys. The player object of the YouTube TV page has the methods
//! of the YouTube IFrame player, for example getPlayerState and seekTo.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{ApiError, Result};
use crate::status::{PlayState, Running, Source, Status};

pub const DEBUG_PORT: u16 = 47811;
const TIMEOUT: Duration = Duration::from_secs(3);

const PLAYER_JS: &str = r#"[document.getElementById("movie_player"),
  document.querySelector(".html5-video-player")].find(e => e && e.getPlayerState)"#;

/// True while VacuumTube runs, because only VacuumTube opens the port.
pub async fn running() -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_millis(300),
            TcpStream::connect(("127.0.0.1", DEBUG_PORT))
        )
        .await,
        Ok(Ok(_))
    )
}

/// Runs one JavaScript expression in the page and returns its value.
async fn evaluate(expression: &str) -> Result<Value> {
    tokio::time::timeout(TIMEOUT, evaluate_inner(expression))
        .await
        .map_err(|_| ApiError::backend("VacuumTube did not answer in time."))?
}

async fn evaluate_inner(expression: &str) -> Result<Value> {
    let unreachable =
        |e: &dyn std::fmt::Display| ApiError::offline(format!("VacuumTube does not answer: {e}"));
    let targets: Value = reqwest::get(format!("http://127.0.0.1:{DEBUG_PORT}/json"))
        .await
        .map_err(|e| unreachable(&e))?
        .json()
        .await
        .map_err(|e| unreachable(&e))?;
    let socket_url = targets
        .as_array()
        .and_then(|t| t.iter().find(|t| t["type"] == "page"))
        .and_then(|t| t["webSocketDebuggerUrl"].as_str())
        .ok_or_else(|| ApiError::offline("VacuumTube has no page."))?
        .to_string();
    let (mut socket, _) = tokio_tungstenite::connect_async(socket_url)
        .await
        .map_err(|e| unreachable(&e))?;
    let request = json!({"id": 1, "method": "Runtime.evaluate",
                         "params": {"expression": expression, "returnByValue": true}});
    socket
        .send(Message::text(request.to_string()))
        .await
        .map_err(|e| unreachable(&e))?;
    while let Some(message) = socket.next().await {
        let Ok(Message::Text(text)) = message else {
            continue;
        };
        let Ok(answer) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if answer["id"] == 1 {
            let _ = socket.close(None).await;
            return Ok(answer["result"]["result"]["value"].clone());
        }
    }
    Err(ApiError::backend("VacuumTube closed the connection."))
}

/// The video that plays, or None in the menus
pub async fn status() -> Result<Option<Status>> {
    let script = format!(
        r##"(() => {{
          const p = {PLAYER_JS};
          if (!p || !location.hash.startsWith("#/watch")) return null;
          const d = p.getVideoData ? p.getVideoData() : {{}};
          return {{state: p.getPlayerState(), time: p.getCurrentTime(), duration: p.getDuration(),
                   id: d.video_id, title: d.title, author: d.author}};
        }})()"##
    );
    let value = evaluate(&script).await?;
    if !value.is_object() {
        return Ok(None);
    }
    // The states of the IFrame player: -1 unstarted, 0 ended, 1 playing,
    // 2 paused, 3 buffering, 5 cued.
    let state = match value["state"].as_i64() {
        Some(1) => PlayState::Playing,
        Some(2) => PlayState::Paused,
        Some(3) | Some(-1) | Some(5) => PlayState::Buffering,
        _ => return Ok(None),
    };
    let id = value["id"]
        .as_str()
        .filter(|i| !i.is_empty())
        .map(String::from);
    let text = |key: &str| {
        value[key]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(String::from)
    };
    let (mut title, mut author) = (text("title"), text("author"));
    if title.is_none()
        && let Some(id) = &id
    {
        (title, author) = video_names(id).await;
    }
    Ok(Some(Status {
        source: Source::Youtube,
        state,
        title,
        subtitle: author,
        position: value["time"].as_f64(),
        duration: value["duration"].as_f64().filter(|d| *d > 0.0),
        image: id
            .as_ref()
            .map(|id| format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
        url: id.as_ref().map(|id| watch_url(id, None)),
        media_id: id,
        running: Running::default(),
    }))
}

/// The title and the channel of a video. The TV interface of YouTube gives
/// the player no title, so the names come from the public oEmbed address
/// of YouTube, once for each video.
async fn video_names(video_id: &str) -> (Option<String>, Option<String>) {
    type Names = (Option<String>, Option<String>);
    static CACHE: std::sync::Mutex<Option<(String, Names)>> = std::sync::Mutex::new(None);
    if let Some((id, names)) = CACHE.lock().unwrap().as_ref()
        && id == video_id
    {
        return names.clone();
    }
    let url = format!(
        "https://www.youtube.com/oembed?format=json&url={}",
        watch_url(video_id, None)
    );
    let answer: Option<Value> = async {
        let response = reqwest::Client::new()
            .get(url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        response.error_for_status().ok()?.json().await.ok()
    }
    .await;
    let names = answer
        .map(|a| {
            let text = |key: &str| a[key].as_str().map(String::from);
            (text("title"), text("author_name"))
        })
        .unwrap_or_default();
    *CACHE.lock().unwrap() = Some((video_id.to_string(), names.clone()));
    names
}

async fn player_call(call: &str) -> Result<()> {
    let script = format!(
        "(() => {{ const p = {PLAYER_JS}; if (!p) return false; {call}; return true; }})()"
    );
    match evaluate(&script).await? {
        Value::Bool(true) => Ok(()),
        _ => Err(ApiError::nothing_plays()),
    }
}

pub async fn pause() -> Result<()> {
    player_call("p.pauseVideo()").await
}

pub async fn resume() -> Result<()> {
    player_call("p.playVideo()").await
}

pub async fn seek(position: f64) -> Result<()> {
    player_call(&format!("p.seekTo({position}, true)")).await
}

/// Ends the video and goes back to the start page of YouTube. VacuumTube
/// keeps running, so the next video starts at once.
pub async fn stop() -> Result<()> {
    player_call(r##"p.stopVideo(); location.hash = "#/""##).await
}

pub fn watch_url(video_id: &str, start: Option<f64>) -> String {
    match start {
        Some(s) if s >= 1.0 => {
            format!("https://www.youtube.com/watch?v={video_id}&t={}s", s as u64)
        }
        _ => format!("https://www.youtube.com/watch?v={video_id}"),
    }
}

/// The video ID of a YouTube address: watch?v=ID, youtu.be/ID, /shorts/ID,
/// /live/ID or /embed/ID
pub fn video_id_from_url(url: &reqwest::Url) -> Option<String> {
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .trim_start_matches("m.");
    let id = match host {
        "youtu.be" => url.path_segments()?.next().map(String::from),
        "youtube.com" | "music.youtube.com" | "youtube-nocookie.com" => {
            let mut segments = url.path_segments()?;
            match segments.next() {
                Some("watch") => url
                    .query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned()),
                Some("shorts" | "live" | "embed" | "v") => segments.next().map(String::from),
                _ => None,
            }
        }
        _ => None,
    }?;
    valid_video_id(&id).then_some(id)
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

    fn id(url: &str) -> Option<String> {
        video_id_from_url(&reqwest::Url::parse(url).unwrap())
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
    fn writes_start_times() {
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
