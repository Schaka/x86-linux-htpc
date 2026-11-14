// SPDX-License-Identifier: AGPL-3.0-only
//! Plays Jellyfin items on jellyfin-mpv-shim through the Jellyfin server.
//!
//! The shim has no local API. It is a session on the Jellyfin server, and
//! the server sends it play commands. This service uses the login of the
//! shim, so it needs no own API key. The request carries only the token:
//! the server then takes the client and device names of the token, so the
//! requests count as the shim itself and add no second session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Url;
use serde_json::Value;

use crate::error::{ApiError, Result};
use crate::status::{PlayState, Running, Source, Status};

const TICKS_PER_SECOND: f64 = 10_000_000.0;
const SHIM_DIR: &str = ".config/jellyfin-mpv-shim";
/// The login of the add-on "Jellyfin for Kodi"
const KODI_ADDON_DATA: &str = ".kodi/userdata/addon_data/plugin.video.jellyfin/data.json";

pub struct Jellyfin {
    http: reqwest::Client,
    shim_dir: PathBuf,
    kodi_data: PathBuf,
}

struct Login {
    server: Url,
    token: String,
    user_id: String,
    device_id: String,
}

impl std::fmt::Debug for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token stays out of the logs.
        f.debug_struct("Login")
            .field("server", &self.server.as_str())
            .finish_non_exhaustive()
    }
}

pub struct Session {
    id: String,
    value: Value,
}

impl Jellyfin {
    pub fn new(http: reqwest::Client, home: &Path) -> Jellyfin {
        Jellyfin {
            http,
            shim_dir: home.join(SHIM_DIR),
            kodi_data: home.join(KODI_ADDON_DATA),
        }
    }

    /// The shim writes its login to users.json. Older versions of the shim
    /// write cred.json. The file is read for each request, because the shim
    /// writes a new token at a new login.
    fn login(&self) -> Result<Login> {
        let read = |name: &str| -> Option<Value> {
            let text = std::fs::read_to_string(self.shim_dir.join(name)).ok()?;
            serde_json::from_str(&text).ok()
        };
        let not_logged_in = || ApiError::offline("jellyfin-mpv-shim has no Jellyfin login.");
        let (credential, device_id) = if let Some(users) = read("users.json") {
            let active = users["active"].as_str().unwrap_or_default();
            let list = users["users"].as_array().cloned().unwrap_or_default();
            let user = list
                .iter()
                .find(|u| u["id"].as_str() == Some(active))
                .or_else(|| list.first())
                .ok_or_else(not_logged_in)?;
            let credential = user["credentials"]
                .as_array()
                .and_then(|c| c.iter().find(|c| c["AccessToken"].is_string()))
                .cloned()
                .ok_or_else(not_logged_in)?;
            (credential, user["device_id"].as_str().map(String::from))
        } else {
            let credentials = read("cred.json").ok_or_else(not_logged_in)?;
            let credential = credentials
                .as_array()
                .and_then(|c| c.iter().find(|c| c["AccessToken"].is_string()))
                .cloned()
                .ok_or_else(not_logged_in)?;
            (credential, None)
        };
        let device_id = device_id
            .or_else(|| read("conf.json").and_then(|c| c["client_uuid"].as_str().map(String::from)))
            .ok_or_else(not_logged_in)?;
        let text = |key: &str| {
            credential[key]
                .as_str()
                .map(String::from)
                .ok_or_else(not_logged_in)
        };
        let server = Url::parse(&text("address")?).map_err(|_| {
            ApiError::offline("The Jellyfin address of jellyfin-mpv-shim is not valid.")
        })?;
        Ok(Login {
            server,
            token: text("AccessToken")?,
            user_id: text("UserId")?,
            device_id,
        })
    }

    fn request(
        &self,
        login: &Login,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let mut url = login.server.clone();
        let base = url.path().trim_end_matches('/').to_string();
        url.set_path(&format!("{base}{path}"));
        self.http
            .request(method, url)
            .header(
                "Authorization",
                format!("MediaBrowser Token=\"{}\"", login.token),
            )
            .timeout(Duration::from_secs(5))
    }

    async fn session(&self, login: &Login) -> Result<Option<Session>> {
        let sessions: Value = self
            .request(login, reqwest::Method::GET, "/Sessions")
            .query(&[("deviceId", login.device_id.as_str())])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(sessions
            .as_array()
            .and_then(|list| list.first())
            .map(|s| Session {
                id: s["Id"].as_str().unwrap_or_default().to_string(),
                value: s.clone(),
            }))
    }

    async fn online_session(&self, login: &Login) -> Result<Session> {
        self.session(login).await?.ok_or_else(|| {
            ApiError::offline("The session of jellyfin-mpv-shim is not on the Jellyfin server.")
        })
    }

    /// The media that the shim plays, or None
    pub async fn status(&self) -> Result<Option<Status>> {
        let login = self.login()?;
        let Some(session) = self.session(&login).await? else {
            return Ok(None);
        };
        let item = &session.value["NowPlayingItem"];
        if !item.is_object() {
            return Ok(None);
        }
        let play_state = &session.value["PlayState"];
        let seconds = |v: &Value| v.as_f64().map(|t| t / TICKS_PER_SECOND);
        let id = item["Id"].as_str().map(String::from);
        let mut image = None;
        if let Some(id) = &id {
            let mut url = login.server.clone();
            let base = url.path().trim_end_matches('/').to_string();
            url.set_path(&format!("{base}/Items/{id}/Images/Primary"));
            image = Some(url.to_string());
        }
        Ok(Some(Status {
            source: Source::Jellyfin,
            state: if play_state["IsPaused"].as_bool() == Some(true) {
                PlayState::Paused
            } else {
                PlayState::Playing
            },
            title: item["Name"].as_str().map(String::from),
            subtitle: episode_line(item),
            position: seconds(&play_state["PositionTicks"]),
            duration: seconds(&item["RunTimeTicks"]),
            media_id: id,
            url: None,
            image,
            running: Running::default(),
        }))
    }

    /// The resume position of the item for the user of the shim
    /// The resume position of the item for the Jellyfin user of the shim
    pub async fn resume_position(&self, item_id: &str) -> Result<Option<f64>> {
        let login = self.login()?;
        self.resume_position_of(&login, item_id).await
    }

    /// The resume position of the item for the Jellyfin user of the Kodi
    /// add-on. The add-on can use another user than the shim, and it does
    /// not resume by itself when it gets an item to play.
    pub async fn kodi_resume_position(&self, item_id: &str) -> Result<Option<f64>> {
        let text = std::fs::read_to_string(&self.kodi_data)
            .map_err(|_| ApiError::offline("The Kodi add-on Jellyfin for Kodi has no login."))?;
        let data: Value = serde_json::from_str(&text)
            .map_err(|_| ApiError::offline("The login file of the Kodi add-on is not valid."))?;
        let server = &data["Servers"][0];
        let text = |key: &str| server[key].as_str().map(String::from);
        let (Some(address), Some(token), Some(user_id)) =
            (text("address"), text("AccessToken"), text("UserId"))
        else {
            return Err(ApiError::offline(
                "The Kodi add-on Jellyfin for Kodi has no login.",
            ));
        };
        let server = Url::parse(&address).map_err(|_| {
            ApiError::offline("The Jellyfin address of the Kodi add-on is not valid.")
        })?;
        let login = Login {
            server,
            token,
            user_id,
            device_id: String::new(),
        };
        self.resume_position_of(&login, item_id).await
    }

    async fn resume_position_of(&self, login: &Login, item_id: &str) -> Result<Option<f64>> {
        let path = format!("/Users/{}/Items/{}", login.user_id, item_id);
        let response = self
            .request(login, reqwest::Method::GET, &path)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND
            || response.status() == reqwest::StatusCode::BAD_REQUEST
        {
            return Err(ApiError::not_found(format!(
                "Jellyfin has no item {item_id}."
            )));
        }
        let item: Value = response.error_for_status()?.json().await?;
        let ticks = item["UserData"]["PlaybackPositionTicks"]
            .as_f64()
            .unwrap_or(0.0);
        Ok((ticks > 0.0).then_some(ticks / TICKS_PER_SECOND))
    }

    pub async fn play(&self, item_id: &str, start: Option<f64>) -> Result<()> {
        let login = self.login()?;
        let session = self.online_session(&login).await?;
        let mut query = vec![
            ("playCommand", "PlayNow".to_string()),
            ("itemIds", item_id.to_string()),
        ];
        if let Some(start) = start {
            query.push((
                "startPositionTicks",
                ((start * TICKS_PER_SECOND) as i64).to_string(),
            ));
        }
        self.request(
            &login,
            reqwest::Method::POST,
            &format!("/Sessions/{}/Playing", session.id),
        )
        .query(&query)
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    /// A play state command: Pause, Unpause, PlayPause or Stop
    pub async fn command(&self, command: &str) -> Result<()> {
        let login = self.login()?;
        let session = self.online_session(&login).await?;
        self.request(
            &login,
            reqwest::Method::POST,
            &format!("/Sessions/{}/Playing/{command}", session.id),
        )
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        let login = self.login()?;
        let session = self.online_session(&login).await?;
        let ticks = ((position * TICKS_PER_SECOND) as i64).to_string();
        self.request(
            &login,
            reqwest::Method::POST,
            &format!("/Sessions/{}/Playing/Seek", session.id),
        )
        .query(&[("seekPositionTicks", ticks)])
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    /// The item ID of a stream address of the Jellyfin server of the shim,
    /// for example /Videos/ID/stream or /Items/ID/Download. Home Assistant
    /// and DLNA apps send such addresses. The shim then plays the item
    /// itself, and Jellyfin keeps the watched state and the resume position.
    pub fn item_id_from_url(&self, url: &Url) -> Option<String> {
        let login = self.login().ok()?;
        if url.host_str() != login.server.host_str()
            || url.port_or_known_default() != login.server.port_or_known_default()
        {
            return None;
        }
        item_id_from_path(url.path())
    }
}

fn item_id_from_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    parts.windows(2).find_map(|pair| {
        let kind = pair[0].to_ascii_lowercase();
        let id = pair[1];
        let is_id = id.len() >= 32 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        (matches!(kind.as_str(), "videos" | "items" | "audio") && is_id)
            .then(|| id.replace('-', "").to_lowercase())
    })
}

/// "Series, S1 E2" for an episode
fn episode_line(item: &Value) -> Option<String> {
    let series = item["SeriesName"].as_str()?;
    match (
        item["ParentIndexNumber"].as_i64(),
        item["IndexNumber"].as_i64(),
    ) {
        (Some(season), Some(episode)) => Some(format!("{series}, S{season} E{episode}")),
        _ => Some(series.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_item_ids_in_stream_paths() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            item_id_from_path(&format!("/Videos/{id}/stream.mkv")).as_deref(),
            Some(id)
        );
        assert_eq!(
            item_id_from_path(&format!("/jellyfin/Items/{id}/Download")).as_deref(),
            Some(id)
        );
        assert_eq!(
            item_id_from_path(&format!("/Audio/{id}/universal")).as_deref(),
            Some(id)
        );
        assert_eq!(item_id_from_path("/Videos/short/stream"), None);
        assert_eq!(item_id_from_path("/web/index.html"), None);
    }

    #[test]
    fn writes_episode_lines() {
        let episode =
            serde_json::json!({"SeriesName": "Show", "ParentIndexNumber": 1, "IndexNumber": 2});
        assert_eq!(episode_line(&episode).as_deref(), Some("Show, S1 E2"));
        assert_eq!(episode_line(&serde_json::json!({"Name": "Film"})), None);
    }
}
