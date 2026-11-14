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

use crate::error::{ApiError, Result};
use crate::status::{PlayState, Running, Source, Status};

const ADDRESS: (&str, u16) = ("127.0.0.1", 9090);
const TIMEOUT: Duration = Duration::from_secs(3);

pub async fn running() -> bool {
    matches!(
        tokio::time::timeout(Duration::from_millis(300), TcpStream::connect(ADDRESS)).await,
        Ok(Ok(_))
    )
}

pub async fn call(method: &str, params: Value) -> Result<Value> {
    tokio::time::timeout(TIMEOUT, call_inner(method, params))
        .await
        .map_err(|_| ApiError::backend("Kodi did not answer in time."))?
}

async fn call_inner(method: &str, params: Value) -> Result<Value> {
    let mut stream = TcpStream::connect(ADDRESS)
        .await
        .map_err(|_| ApiError::offline("Kodi does not run."))?;
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    stream.write_all(request.to_string().as_bytes()).await?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(ApiError::backend("Kodi closed the connection."));
        }
        buffer.extend_from_slice(&chunk[..read]);
        let mut objects = serde_json::Deserializer::from_slice(&buffer).into_iter::<Value>();
        let mut consumed = 0;
        while let Some(Ok(object)) = objects.next() {
            consumed = objects.byte_offset();
            if object["id"] == 1 {
                if let Some(error) = object.get("error") {
                    return Err(ApiError::backend(format!("Kodi refused {method}: {error}")));
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
        .and_then(|p| p.first())
        .and_then(|p| p["playerid"].as_i64()))
}

async fn required_player() -> Result<i64> {
    active_player().await?.ok_or_else(ApiError::nothing_plays)
}

fn seconds(time: &Value) -> Option<f64> {
    let part = |k: &str| time[k].as_f64().unwrap_or(0.0);
    time.is_object().then(|| {
        part("hours") * 3600.0
            + part("minutes") * 60.0
            + part("seconds")
            + part("milliseconds") / 1000.0
    })
}

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
    let title = item["title"]
        .as_str()
        .filter(|t| !t.is_empty())
        .or(item["label"].as_str())
        .map(String::from);
    let subtitle = item["showtitle"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(
            |show| match (item["season"].as_i64(), item["episode"].as_i64()) {
                (Some(s), Some(e)) if s >= 0 && e >= 0 => format!("{show}, S{s} E{e}"),
                _ => show.to_string(),
            },
        );
    Ok(Some(Status {
        source: Source::Kodi,
        state: if properties["speed"].as_f64() == Some(0.0) {
            PlayState::Paused
        } else {
            PlayState::Playing
        },
        title,
        subtitle,
        position: seconds(&properties["time"]),
        duration: seconds(&properties["totaltime"]).filter(|d| *d > 0.0),
        media_id: None,
        url: None,
        image: None,
        running: Running::default(),
    }))
}

pub async fn open(file: &str) -> Result<()> {
    call("Player.Open", json!({"item": {"file": file}}))
        .await
        .map(|_| ())
}

pub async fn set_paused(paused: Option<bool>) -> Result<()> {
    let player = required_player().await?;
    let play = match paused {
        Some(paused) => json!(!paused),
        None => json!("toggle"),
    };
    call(
        "Player.PlayPause",
        json!({"playerid": player, "play": play}),
    )
    .await
    .map(|_| ())
}

pub async fn stop() -> Result<()> {
    let player = required_player().await?;
    call("Player.Stop", json!({"playerid": player}))
        .await
        .map(|_| ())
}

pub async fn seek(position: f64) -> Result<()> {
    let player = required_player().await?;
    let total = position.max(0.0);
    let time = json!({"hours": (total / 3600.0) as i64, "minutes": ((total % 3600.0) / 60.0) as i64,
                      "seconds": (total % 60.0) as i64, "milliseconds": ((total.fract()) * 1000.0) as i64});
    call(
        "Player.Seek",
        json!({"playerid": player, "value": {"time": time}}),
    )
    .await
    .map(|_| ())
}

/// Kodi then exits, and its launcher switches back to Plasma.
pub async fn quit() -> Result<()> {
    call("Application.Quit", json!({})).await.map(|_| ())
}
