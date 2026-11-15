// SPDX-License-Identifier: AGPL-3.0-only
//! Controls Kodi through its JSON-RPC interface on TCP port 9090.
//!
//! The component kodi-dv lets Kodi accept JSON-RPC from localhost only.
//! The TCP interface sends JSON objects without a separator, and also
//! sends notifications between the answers, so the reader takes one
//! complete object after the other and keeps the one with its ID.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{Error, Result};
use crate::status::{PlayState, Source, Status};

const PORT: u16 = 9090;
const TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_ID: i64 = 1;

pub async fn running() -> bool {
    super::port_open(PORT).await
}

pub async fn call(method: &str, params: Value) -> Result<Value> {
    tokio::time::timeout(TIMEOUT, call_without_timeout(method, params))
        .await
        .map_err(|_| Error::player("Kodi did not answer in time."))?
}

async fn call_without_timeout(method: &str, params: Value) -> Result<Value> {
    let mut stream = TcpStream::connect(("127.0.0.1", PORT))
        .await
        .map_err(|_| Error::offline("Kodi does not run."))?;
    let request = json!({"jsonrpc": "2.0", "id": REQUEST_ID, "method": method, "params": params});
    stream.write_all(request.to_string().as_bytes()).await?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(Error::player("Kodi closed the connection."));
        }
        buffer.extend_from_slice(&chunk[..read]);
        let mut objects = serde_json::Deserializer::from_slice(&buffer).into_iter::<Value>();
        let mut consumed = 0;
        // The last object can be incomplete. It stays in the buffer until
        // the next read completes it.
        while let Some(Ok(object)) = objects.next() {
            consumed = objects.byte_offset();
            if object["id"] == REQUEST_ID {
                if let Some(error) = object.get("error") {
                    return Err(Error::player(format!("Kodi refused {method}: {error}")));
                }
                return Ok(object["result"].clone());
            }
        }
        buffer.drain(..consumed);
    }
}

async fn active_player() -> Result<Option<i64>> {
    let players = call("Player.GetActivePlayers", json!({})).await?;
    Ok(players
        .as_array()
        .and_then(|players| players.first())
        .and_then(|player| player["playerid"].as_i64()))
}

async fn required_player() -> Result<i64> {
    active_player().await?.ok_or_else(Error::nothing_plays)
}

/// The media that Kodi plays, or None in the menus
pub async fn status() -> Result<Option<Status>> {
    let Some(player) = active_player().await? else {
        return Ok(None);
    };
    let properties = call(
        "Player.GetProperties",
        json!({"playerid": player, "properties": ["time", "totaltime", "speed"]}),
    )
    .await?;
    let item = call(
        "Player.GetItem",
        json!({"playerid": player, "properties": ["title", "showtitle", "season", "episode", "file"]}),
    )
    .await?;
    let item = &item["item"];
    let title = non_empty(&item["title"])
        .or_else(|| item["label"].as_str())
        .map(String::from);
    let state = if properties["speed"].as_f64() == Some(0.0) {
        PlayState::Paused
    } else {
        PlayState::Playing
    };
    Ok(Some(Status {
        source: Source::Kodi,
        state,
        title,
        subtitle: episode_line(item),
        position: seconds(&properties["time"]),
        duration: seconds(&properties["totaltime"]).filter(|d| *d > 0.0),
        ..Status::default()
    }))
}

pub async fn open(file: &str) -> Result<()> {
    call("Player.Open", json!({"item": {"file": file}})).await?;
    Ok(())
}

pub async fn set_paused(paused: bool) -> Result<()> {
    let player = required_player().await?;
    call(
        "Player.PlayPause",
        json!({"playerid": player, "play": !paused}),
    )
    .await?;
    Ok(())
}

pub async fn stop() -> Result<()> {
    let player = required_player().await?;
    call("Player.Stop", json!({"playerid": player})).await?;
    Ok(())
}

pub async fn seek(position: f64) -> Result<()> {
    let player = required_player().await?;
    call(
        "Player.Seek",
        json!({"playerid": player, "value": {"time": kodi_time(position)}}),
    )
    .await?;
    Ok(())
}

/// Kodi then exits, and its launcher switches back to Plasma.
pub async fn quit() -> Result<()> {
    call("Application.Quit", json!({})).await?;
    Ok(())
}

/// The address of a Jellyfin item in the add-on "Jellyfin for Kodi". The
/// add-on starts at the position of the parameter timestamp, in seconds.
/// Without it, the add-on starts at the beginning.
pub fn jellyfin_addon_file(item_id: &str, start: Option<f64>) -> String {
    let id = item_id.replace('-', "");
    match start.filter(|start| *start >= 1.0) {
        Some(start) => format!(
            "plugin://plugin.video.jellyfin/?mode=play&id={id}&timestamp={}",
            start as u64
        ),
        None => format!("plugin://plugin.video.jellyfin/?mode=play&id={id}"),
    }
}

fn non_empty(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

/// "Series, S1 E2" for an episode. Kodi gives -1 for an unknown season or
/// episode.
fn episode_line(item: &Value) -> Option<String> {
    let show = non_empty(&item["showtitle"])?;
    match (item["season"].as_i64(), item["episode"].as_i64()) {
        (Some(season), Some(episode)) if season >= 0 && episode >= 0 => {
            Some(format!("{show}, S{season} E{episode}"))
        }
        _ => Some(show.to_string()),
    }
}

/// The seconds of a Kodi time object
fn seconds(time: &Value) -> Option<f64> {
    let part = |key: &str| time[key].as_f64().unwrap_or(0.0);
    time.is_object().then(|| {
        part("hours") * 3600.0
            + part("minutes") * 60.0
            + part("seconds")
            + part("milliseconds") / 1000.0
    })
}

/// A Kodi time object for a position in seconds
fn kodi_time(position: f64) -> Value {
    // The cast saturates, so a negative position becomes 0.
    let millis = (position * 1000.0).round() as u64;
    json!({
        "hours": millis / 3_600_000,
        "minutes": millis / 60_000 % 60,
        "seconds": millis / 1000 % 60,
        "milliseconds": millis % 1000,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_times() {
        let time = kodi_time(3725.25);
        assert_eq!(
            time,
            json!({"hours": 1, "minutes": 2, "seconds": 5, "milliseconds": 250})
        );
        assert_eq!(seconds(&time), Some(3725.25));
        assert_eq!(seconds(&Value::Null), None);
    }

    #[test]
    fn writes_addon_files() {
        assert_eq!(
            jellyfin_addon_file("0123-abcd", Some(90.7)),
            "plugin://plugin.video.jellyfin/?mode=play&id=0123abcd&timestamp=90"
        );
        assert_eq!(
            jellyfin_addon_file("0123abcd", Some(0.5)),
            "plugin://plugin.video.jellyfin/?mode=play&id=0123abcd"
        );
    }

    #[test]
    fn writes_episode_lines() {
        let episode = json!({"showtitle": "Show", "season": 1, "episode": 2});
        assert_eq!(episode_line(&episode).as_deref(), Some("Show, S1 E2"));
        let movie = json!({"showtitle": "", "season": -1, "episode": -1});
        assert_eq!(episode_line(&movie), None);
    }
}
