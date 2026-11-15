// SPDX-License-Identifier: AGPL-3.0-only
//! Plays Jellyfin items on jellyfin-mpv-shim through the Jellyfin server.
//!
//! The shim has no local API. It is a session on the Jellyfin server, and
//! the server sends it play commands. This service uses the login of the
//! shim, so it needs no own API key. The request carries only the token:
//! the server then takes the client and device names of the token, so the
//! requests count as the shim itself and add no second session.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::status::{PlayState, Source, Status};

const TICKS_PER_SECOND: f64 = 10_000_000.0;
const TIMEOUT: Duration = Duration::from_secs(5);
const SHIM_DIR: &str = ".config/jellyfin-mpv-shim";
/// The login of the add-on "Jellyfin for Kodi"
const KODI_ADDON_DATA: &str = ".kodi/userdata/addon_data/plugin.video.jellyfin/data.json";

/// A play state command of the Jellyfin remote control
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayCommand {
    Pause,
    Unpause,
    Stop,
}

impl PlayCommand {
    fn as_str(self) -> &'static str {
        match self {
            PlayCommand::Pause => "Pause",
            PlayCommand::Unpause => "Unpause",
            PlayCommand::Stop => "Stop",
        }
    }
}

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

impl fmt::Debug for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The token stays out of the logs.
        f.debug_struct("Login")
            .field("server", &self.server.as_str())
            .finish_non_exhaustive()
    }
}

/// The session of the shim on the Jellyfin server
struct Session {
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

    fn read_shim_file(&self, name: &str) -> Option<Value> {
        let text = std::fs::read_to_string(self.shim_dir.join(name)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The shim writes its login to users.json. Older versions of the shim
    /// write cred.json. The file is read for each request, because the shim
    /// writes a new token at a new login.
    fn login(&self) -> Result<Login> {
        let not_logged_in = || Error::offline("jellyfin-mpv-shim has no Jellyfin login.");
        let (credential, device_id) = match self.read_shim_file("users.json") {
            Some(users) => {
                let user = active_user(&users).ok_or_else(not_logged_in)?;
                let credential =
                    first_credential(&user["credentials"]).ok_or_else(not_logged_in)?;
                (credential, user["device_id"].as_str().map(String::from))
            }
            None => {
                let credentials = self.read_shim_file("cred.json").ok_or_else(not_logged_in)?;
                (
                    first_credential(&credentials).ok_or_else(not_logged_in)?,
                    None,
                )
            }
        };
        let device_id = device_id
            .or_else(|| {
                self.read_shim_file("conf.json")
                    .and_then(|conf| conf["client_uuid"].as_str().map(String::from))
            })
            .ok_or_else(not_logged_in)?;
        let text = |key: &str| {
            credential[key]
                .as_str()
                .map(String::from)
                .ok_or_else(not_logged_in)
        };
        let server = Url::parse(&text("address")?).map_err(|_| {
            Error::offline("The Jellyfin address of jellyfin-mpv-shim is not valid.")
        })?;
        Ok(Login {
            server,
            token: text("AccessToken")?,
            user_id: text("UserId")?,
            device_id,
        })
    }

    fn request(&self, login: &Login, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, endpoint(&login.server, path))
            .header(
                "Authorization",
                format!("MediaBrowser Token=\"{}\"", login.token),
            )
            .timeout(TIMEOUT)
    }

    async fn session(&self, login: &Login) -> Result<Option<Session>> {
        let sessions: Value = self
            .request(login, Method::GET, "/Sessions")
            .query(&[("deviceId", login.device_id.as_str())])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(sessions
            .as_array()
            .and_then(|list| list.first())
            .map(|session| Session {
                id: session["Id"].as_str().unwrap_or_default().to_string(),
                value: session.clone(),
            }))
    }

    async fn online_session(&self, login: &Login) -> Result<Session> {
        self.session(login).await?.ok_or_else(|| {
            Error::offline("The session of jellyfin-mpv-shim is not on the Jellyfin server.")
        })
    }

    /// Sends a command to the session of the shim. The path follows
    /// /Sessions/ID/Playing.
    async fn send_to_session(&self, path: &str, query: &[(&str, String)]) -> Result<()> {
        let login = self.login()?;
        let session = self.online_session(&login).await?;
        self.request(
            &login,
            Method::POST,
            &format!("/Sessions/{}/Playing{path}", session.id),
        )
        .query(query)
        .send()
        .await?
        .error_for_status()?;
        Ok(())
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
        let id = item["Id"].as_str().map(String::from);
        let state = if play_state["IsPaused"].as_bool() == Some(true) {
            PlayState::Paused
        } else {
            PlayState::Playing
        };
        Ok(Some(Status {
            source: Source::Jellyfin,
            state,
            title: item["Name"].as_str().map(String::from),
            subtitle: episode_line(item),
            position: play_state["PositionTicks"].as_f64().map(seconds),
            duration: item["RunTimeTicks"].as_f64().map(seconds),
            image: id
                .as_ref()
                .map(|id| endpoint(&login.server, &format!("/Items/{id}/Images/Primary")).into()),
            media_id: id,
            ..Status::default()
        }))
    }

    /// The resume position of the item for the Jellyfin user of the shim
    pub async fn resume_position(&self, item_id: &str) -> Result<Option<f64>> {
        let login = self.login()?;
        self.resume_position_of(&login, item_id).await
    }

    /// The resume position of the item for the Jellyfin user of the Kodi
    /// add-on. The add-on can use another user than the shim, and it does
    /// not resume by itself when it gets an item to play.
    pub async fn kodi_resume_position(&self, item_id: &str) -> Result<Option<f64>> {
        let login = self.kodi_login()?;
        self.resume_position_of(&login, item_id).await
    }

    fn kodi_login(&self) -> Result<Login> {
        let no_login = || Error::offline("The Kodi add-on Jellyfin for Kodi has no login.");
        let text = std::fs::read_to_string(&self.kodi_data).map_err(|_| no_login())?;
        let data: Value = serde_json::from_str(&text)
            .map_err(|_| Error::offline("The login file of the Kodi add-on is not valid."))?;
        let server = &data["Servers"][0];
        let value = |key: &str| server[key].as_str().map(String::from).ok_or_else(no_login);
        let address = value("address")?;
        let server = Url::parse(&address)
            .map_err(|_| Error::offline("The Jellyfin address of the Kodi add-on is not valid."))?;
        Ok(Login {
            server,
            token: value("AccessToken")?,
            user_id: value("UserId")?,
            device_id: String::new(),
        })
    }

    async fn resume_position_of(&self, login: &Login, item_id: &str) -> Result<Option<f64>> {
        let path = format!("/Users/{}/Items/{item_id}", login.user_id);
        let response = self.request(login, Method::GET, &path).send().await?;
        if matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST
        ) {
            return Err(Error::not_found(format!("Jellyfin has no item {item_id}.")));
        }
        let item: Value = response.error_for_status()?.json().await?;
        let ticks = item["UserData"]["PlaybackPositionTicks"]
            .as_f64()
            .unwrap_or(0.0);
        Ok((ticks > 0.0).then(|| seconds(ticks)))
    }

    pub async fn play(&self, item_id: &str, start: Option<f64>) -> Result<()> {
        let mut query = vec![
            ("playCommand", "PlayNow".to_string()),
            ("itemIds", item_id.to_string()),
        ];
        if let Some(start) = start {
            query.push(("startPositionTicks", ticks(start).to_string()));
        }
        self.send_to_session("", &query).await
    }

    pub async fn command(&self, command: PlayCommand) -> Result<()> {
        self.send_to_session(&format!("/{}", command.as_str()), &[])
            .await
    }

    pub async fn set_paused(&self, paused: bool) -> Result<()> {
        self.command(if paused {
            PlayCommand::Pause
        } else {
            PlayCommand::Unpause
        })
        .await
    }

    pub async fn seek(&self, position: f64) -> Result<()> {
        self.send_to_session(
            "/Seek",
            &[("seekPositionTicks", ticks(position).to_string())],
        )
        .await
    }

    /// The item ID of a stream address of the Jellyfin server of the shim,
    /// for example /Videos/ID/stream or /Items/ID/Download. Home Assistant
    /// and DLNA apps send such addresses. The shim then plays the item
    /// itself, and Jellyfin keeps the watched state and the resume position.
    pub fn item_id_from_url(&self, url: &Url) -> Option<String> {
        let login = self.login().ok()?;
        let same_server = url.host_str() == login.server.host_str()
            && url.port_or_known_default() == login.server.port_or_known_default();
        if !same_server {
            return None;
        }
        item_id_from_path(url.path())
    }
}

/// A Jellyfin item ID: 32 hex digits, with or without the dashes of a UUID
pub fn valid_item_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// The path on the server, also for a server under a base path such as
/// https://example.org/jellyfin
fn endpoint(server: &Url, path: &str) -> Url {
    let mut url = server.clone();
    let base = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!("{base}{path}"));
    url
}

fn seconds(ticks: f64) -> f64 {
    ticks / TICKS_PER_SECOND
}

fn ticks(seconds: f64) -> i64 {
    (seconds * TICKS_PER_SECOND) as i64
}

/// The user that is active in the shim, or else the first user
fn active_user(users: &Value) -> Option<&Value> {
    let active = users["active"].as_str();
    let list = users["users"].as_array()?;
    list.iter()
        .find(|user| user["id"].as_str() == active)
        .or_else(|| list.first())
}

fn first_credential(credentials: &Value) -> Option<Value> {
    credentials
        .as_array()?
        .iter()
        .find(|credential| credential["AccessToken"].is_string())
        .cloned()
}

fn item_id_from_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    parts.windows(2).find_map(|pair| {
        let [kind, id] = pair else { return None };
        let media_path = matches!(
            kind.to_ascii_lowercase().as_str(),
            "videos" | "items" | "audio"
        );
        (media_path && id.len() >= 32 && valid_item_id(id))
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
    use serde_json::json;

    /// A new empty home folder for one test
    fn test_home(name: &str) -> PathBuf {
        let home =
            std::env::temp_dir().join(format!("htpc-control-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(SHIM_DIR)).unwrap();
        home
    }

    fn write(path: PathBuf, value: &Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value.to_string()).unwrap();
    }

    fn credential(address: &str, token: &str) -> Value {
        json!({"address": address, "AccessToken": token, "UserId": "user-1"})
    }

    #[test]
    fn reads_the_login_of_the_active_user() {
        let home = test_home("users");
        write(
            home.join(SHIM_DIR).join("users.json"),
            &json!({"active": "b", "users": [
                {"id": "a", "device_id": "dev-a", "credentials": [credential("http://a:8096", "ta")]},
                {"id": "b", "device_id": "dev-b", "credentials": [{"address": "x"}, credential("http://b:8096", "tb")]},
            ]}),
        );
        let login = Jellyfin::new(reqwest::Client::new(), &home)
            .login()
            .unwrap();
        assert_eq!(login.server.as_str(), "http://b:8096/");
        assert_eq!(login.token, "tb");
        assert_eq!(login.device_id, "dev-b");
        assert_eq!(login.user_id, "user-1");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn reads_the_old_login_file() {
        let home = test_home("cred");
        write(
            home.join(SHIM_DIR).join("cred.json"),
            &json!([credential("https://example.org/jellyfin", "t")]),
        );
        let jellyfin = Jellyfin::new(reqwest::Client::new(), &home);
        // Without conf.json, the shim has no device ID.
        assert_eq!(
            jellyfin.login().unwrap_err().kind,
            crate::error::ErrorKind::Offline
        );
        write(
            home.join(SHIM_DIR).join("conf.json"),
            &json!({"client_uuid": "dev-c"}),
        );
        let login = jellyfin.login().unwrap();
        assert_eq!(login.device_id, "dev-c");
        assert_eq!(
            endpoint(&login.server, "/Sessions").as_str(),
            "https://example.org/jellyfin/Sessions"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn reads_the_login_of_the_kodi_addon() {
        let home = test_home("kodi");
        let jellyfin = Jellyfin::new(reqwest::Client::new(), &home);
        assert!(jellyfin.kodi_login().is_err());
        write(
            home.join(KODI_ADDON_DATA),
            &json!({"Servers": [credential("http://k:8096", "tk")]}),
        );
        let login = jellyfin.kodi_login().unwrap();
        assert_eq!(login.token, "tk");
        assert_eq!(login.server.host_str(), Some("k"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn checks_item_ids() {
        assert!(valid_item_id("0123abcd-ef01"));
        assert!(!valid_item_id(""));
        assert!(!valid_item_id("../etc"));
        assert_eq!(ticks(1.5), 15_000_000);
        assert!((seconds(15_000_000.0) - 1.5).abs() < f64::EPSILON);
    }

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
        let episode = json!({"SeriesName": "Show", "ParentIndexNumber": 1, "IndexNumber": 2});
        assert_eq!(episode_line(&episode).as_deref(), Some("Show, S1 E2"));
        assert_eq!(episode_line(&json!({"Name": "Film"})), None);
    }

    #[test]
    fn keeps_the_base_path_of_the_server() {
        let server = Url::parse("https://example.org/jellyfin/").unwrap();
        assert_eq!(
            endpoint(&server, "/Sessions").as_str(),
            "https://example.org/jellyfin/Sessions"
        );
    }

    #[test]
    fn picks_the_active_user() {
        let users = json!({"active": "b", "users": [{"id": "a"}, {"id": "b"}]});
        assert_eq!(active_user(&users).unwrap()["id"], "b");
        let users = json!({"active": "c", "users": [{"id": "a"}]});
        assert_eq!(active_user(&users).unwrap()["id"], "a");
    }
}
