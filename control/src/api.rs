// SPDX-License-Identifier: AGPL-3.0-only
//! The REST API. The OpenAPI document comes from the annotations in this
//! file: `htpc-control openapi` prints it, and the server serves it at
//! /openapi.json.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::apps::{self, App};
use crate::control::{Controller, JellyfinPlayer};
use crate::error::{Error, ErrorKind};
use crate::players::youtube;
use crate::status::{PlayState, Running, Source, Status};

type Result<T> = std::result::Result<T, Error>;
type SharedController = State<Arc<Controller>>;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "x86-linux-htpc control API",
        description = "Starts media and apps on the HTPC and controls the player that plays. \
                       The API is for the local network and has no authentication.",
        license(name = "AGPL-3.0-only", identifier = "AGPL-3.0-only")
    ),
    paths(
        health, status, list_apps, launch_app, play_youtube, play_jellyfin, play_url,
        pause, resume, toggle, stop, seek, home, wake_tv
    ),
    components(schemas(
        Status, Source, PlayState, Running, App, ErrorBody, Health, Started,
        YoutubeRequest, JellyfinRequest, JellyfinPlayer, UrlRequest, SeekRequest
    )),
    tags(
        (name = "status", description = "What plays"),
        (name = "play", description = "Start media"),
        (name = "playback", description = "Control the player that plays"),
        (name = "apps", description = "The apps of the home screen"),
        (name = "device", description = "The HTPC and the TV")
    )
)]
pub struct ApiDoc;

pub fn router(controller: Arc<Controller>) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .route("/apps", get(list_apps))
        .route("/apps/{id}/launch", post(launch_app))
        .route("/play/youtube", post(play_youtube))
        .route("/play/jellyfin", post(play_jellyfin))
        .route("/play/url", post(play_url))
        .route("/playback/pause", post(pause))
        .route("/playback/resume", post(resume))
        .route("/playback/toggle", post(toggle))
        .route("/playback/stop", post(stop))
        .route("/playback/seek", post(seek))
        .route("/home", post(home))
        .route("/tv/wake", post(wake_tv));
    Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(openapi_json))
        .nest("/api/v1", api)
        .with_state(controller)
}

/// The answer of the API to a failed request
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// A fixed code for scripts, for example not_installed
    pub error: String,
    /// A sentence for a person
    pub message: String,
}

fn status_code(kind: ErrorKind) -> StatusCode {
    match kind {
        ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::NotInstalled | ErrorKind::Offline => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKind::NothingPlays => StatusCode::CONFLICT,
        ErrorKind::Player => StatusCode::BAD_GATEWAY,
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: self.kind.code().to_string(),
            message: self.message,
        };
        (status_code(self.kind), Json(body)).into_response()
    }
}

fn accepted(source: Source) -> (StatusCode, Json<Started>) {
    (StatusCode::ACCEPTED, Json(Started { source }))
}

async fn openapi_json() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}

#[derive(Serialize, ToSchema)]
pub struct Health {
    /// Always ok while the service runs
    status: String,
    version: String,
}

/// The service runs.
#[utoipa::path(get, path = "/health", tag = "device",
    responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

/// The media that plays, its player and its position. The service reads
/// the players every 2 seconds.
#[utoipa::path(get, path = "/api/v1/status", tag = "status",
    responses((status = 200, body = Status)))]
async fn status(State(controller): SharedController) -> Json<Status> {
    Json(controller.status())
}

/// The apps of the home screen
#[utoipa::path(get, path = "/api/v1/apps", tag = "apps",
    responses((status = 200, body = Vec<App>)))]
async fn list_apps(State(controller): SharedController) -> Json<Vec<App>> {
    Json(apps::list(&controller.config))
}

/// Starts an app of the home screen, by its id or its desktop ID. The
/// other players stop first.
#[utoipa::path(post, path = "/api/v1/apps/{id}/launch", tag = "apps",
    params(("id" = String, Path, description = "The id of the app, for example youtube, kodi, jellyfin or s0undtv")),
    responses((status = 200, body = App), (status = 404, body = ErrorBody), (status = 503, body = ErrorBody)))]
async fn launch_app(
    State(controller): SharedController,
    Path(id): Path<String>,
) -> Result<Json<App>> {
    controller.launch_app(&id).await.map(Json)
}

/// The player that the request started
#[derive(Serialize, ToSchema)]
pub struct Started {
    source: Source,
}

#[derive(Deserialize, ToSchema)]
pub struct YoutubeRequest {
    /// The 11 characters of the video ID. Give this or url.
    video_id: Option<String>,
    /// A YouTube address, for example https://youtu.be/ID
    url: Option<String>,
    /// The start in seconds
    start: Option<f64>,
}

/// Plays a YouTube video in VacuumTube.
#[utoipa::path(post, path = "/api/v1/play/youtube", tag = "play", request_body = YoutubeRequest,
    responses((status = 202, body = Started), (status = 400, body = ErrorBody), (status = 503, body = ErrorBody)))]
async fn play_youtube(
    State(controller): SharedController,
    Json(request): Json<YoutubeRequest>,
) -> Result<(StatusCode, Json<Started>)> {
    let video_id = match (request.video_id, request.url) {
        (Some(id), _) => id,
        (None, Some(url)) => reqwest::Url::parse(&url)
            .ok()
            .and_then(|url| youtube::video_id_from_url(&url))
            .ok_or_else(|| Error::bad_request(format!("{url} is not a YouTube video address.")))?,
        (None, None) => return Err(Error::bad_request("Give video_id or url.")),
    };
    controller.play_youtube(&video_id, request.start).await?;
    Ok(accepted(Source::Youtube))
}

fn default_resume() -> bool {
    true
}

#[derive(Deserialize, ToSchema)]
pub struct JellyfinRequest {
    /// The ID of a movie, an episode or a song
    item_id: String,
    /// The start in seconds. Without it, resume decides.
    start: Option<f64>,
    /// Without start: start at the resume position of the Jellyfin user of the player.
    #[serde(default = "default_resume")]
    #[schema(default = true)]
    resume: bool,
    #[serde(default)]
    player: JellyfinPlayer,
}

/// Plays a Jellyfin item, in jellyfin-mpv-shim or in Kodi.
#[utoipa::path(post, path = "/api/v1/play/jellyfin", tag = "play", request_body = JellyfinRequest,
    responses((status = 202, body = Started), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 503, body = ErrorBody)))]
async fn play_jellyfin(
    State(controller): SharedController,
    Json(request): Json<JellyfinRequest>,
) -> Result<(StatusCode, Json<Started>)> {
    controller
        .play_jellyfin(
            &request.item_id,
            request.start,
            request.resume,
            request.player,
        )
        .await?;
    Ok(accepted(request.player.source()))
}

#[derive(Deserialize, ToSchema)]
pub struct UrlRequest {
    /// A media address. A YouTube address plays in VacuumTube, a stream of
    /// the Jellyfin server of the shim in the shim, all others in mpv.
    url: String,
    /// The title for mpv
    title: Option<String>,
    /// True for an audio address without a known extension. mpv then plays
    /// no picture and leaves the TV on Plasma.
    #[serde(default)]
    audio: bool,
}

/// Plays a media address in the player that fits it.
#[utoipa::path(post, path = "/api/v1/play/url", tag = "play", request_body = UrlRequest,
    responses((status = 202, body = Started), (status = 400, body = ErrorBody)))]
async fn play_url(
    State(controller): SharedController,
    Json(request): Json<UrlRequest>,
) -> Result<(StatusCode, Json<Started>)> {
    let source = controller
        .play_url(&request.url, request.title.as_deref(), request.audio)
        .await?;
    Ok(accepted(source))
}

/// Pauses the player that plays.
#[utoipa::path(post, path = "/api/v1/playback/pause", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn pause(State(controller): SharedController) -> Result<StatusCode> {
    controller.set_paused(Some(true)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Continues the paused media.
#[utoipa::path(post, path = "/api/v1/playback/resume", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn resume(State(controller): SharedController) -> Result<StatusCode> {
    controller.set_paused(Some(false)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pauses or continues.
#[utoipa::path(post, path = "/api/v1/playback/toggle", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn toggle(State(controller): SharedController) -> Result<StatusCode> {
    controller.set_paused(None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stops the media. Kodi and VacuumTube keep running.
#[utoipa::path(post, path = "/api/v1/playback/stop", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn stop(State(controller): SharedController) -> Result<StatusCode> {
    controller.stop().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct SeekRequest {
    /// Seconds from the start
    position: f64,
}

/// Goes to a position in the media.
#[utoipa::path(post, path = "/api/v1/playback/seek", tag = "playback", request_body = SeekRequest,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn seek(
    State(controller): SharedController,
    Json(request): Json<SeekRequest>,
) -> Result<StatusCode> {
    controller.seek(request.position).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stops all media and ends Kodi and VacuumTube, so the home screen shows.
#[utoipa::path(post, path = "/api/v1/home", tag = "device", responses((status = 204)))]
async fn home(State(controller): SharedController) -> Result<StatusCode> {
    controller.home().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sends Wake-on-LAN to the TV, with the addresses of WAKE_ON_LAN_MACS.
#[utoipa::path(post, path = "/api/v1/tv/wake", tag = "device",
    responses((status = 204), (status = 503, body = ErrorBody)))]
async fn wake_tv(State(controller): SharedController) -> Result<StatusCode> {
    controller.wake_tv().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_request_takes_an_optional_audio_flag() {
        let plain: UrlRequest = serde_json::from_str(r#"{"url": "http://h/a"}"#).unwrap();
        assert!(!plain.audio);
        assert_eq!(plain.title, None);
        let audio: UrlRequest =
            serde_json::from_str(r#"{"url": "http://h/a", "title": "T", "audio": true}"#).unwrap();
        assert!(audio.audio);
        assert_eq!(audio.title.as_deref(), Some("T"));
    }

    #[test]
    fn documents_all_paths() {
        let doc = ApiDoc::openapi();
        for path in [
            "/health",
            "/api/v1/status",
            "/api/v1/play/youtube",
            "/api/v1/play/jellyfin",
            "/api/v1/apps/{id}/launch",
            "/api/v1/play/url",
            "/api/v1/playback/seek",
            "/api/v1/tv/wake",
        ] {
            assert!(doc.paths.paths.contains_key(path), "{path} is missing");
        }
    }

    #[test]
    fn answers_errors_with_their_status() {
        let response = Error::nothing_plays().into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let response = Error::not_installed("x").into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
