// SPDX-License-Identifier: AGPL-3.0-only
//! The REST API. The OpenAPI document comes from the annotations in this
//! file: `htpc-control openapi` prints it, and the server serves it at
//! /openapi.json.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::apps::{self, App};
use crate::control::{Controller, JellyfinPlayer};
use crate::error::{ApiError, ErrorBody, Result};
use crate::status::{PlayState, Running, Source, Status};

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
async fn status(State(c): State<Arc<Controller>>) -> Json<Status> {
    Json(c.status())
}

/// The apps of the home screen
#[utoipa::path(get, path = "/api/v1/apps", tag = "apps",
    responses((status = 200, body = Vec<App>)))]
async fn list_apps(State(c): State<Arc<Controller>>) -> Json<Vec<App>> {
    Json(apps::list(&c.config))
}

/// Starts an app of the home screen, by its id or its desktop ID. The
/// other players stop first.
#[utoipa::path(post, path = "/api/v1/apps/{id}/launch", tag = "apps",
    params(("id" = String, Path, description = "The id of the app, for example youtube, kodi, jellyfin or s0undtv")),
    responses((status = 200, body = App), (status = 404, body = ErrorBody), (status = 503, body = ErrorBody)))]
async fn launch_app(State(c): State<Arc<Controller>>, Path(id): Path<String>) -> Result<Json<App>> {
    c.launch_app(&id).await.map(Json)
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
    State(c): State<Arc<Controller>>,
    Json(r): Json<YoutubeRequest>,
) -> Result<(StatusCode, Json<Started>)> {
    let video_id = match (r.video_id, r.url) {
        (Some(id), _) => id,
        (None, Some(url)) => reqwest::Url::parse(&url)
            .ok()
            .and_then(|u| crate::youtube::video_id_from_url(&u))
            .ok_or_else(|| {
                ApiError::bad_request(format!("{url} is not a YouTube video address."))
            })?,
        (None, None) => return Err(ApiError::bad_request("Give video_id or url.")),
    };
    c.play_youtube(&video_id, r.start).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Started {
            source: Source::Youtube,
        }),
    ))
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, ToSchema)]
pub struct JellyfinRequest {
    /// The ID of a movie, an episode or a song
    item_id: String,
    /// The start in seconds. Without it, resume decides.
    start: Option<f64>,
    /// Without start: start at the resume position of the Jellyfin user of the shim.
    #[serde(default = "yes")]
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
    State(c): State<Arc<Controller>>,
    Json(r): Json<JellyfinRequest>,
) -> Result<(StatusCode, Json<Started>)> {
    c.play_jellyfin(&r.item_id, r.start, r.resume, r.player)
        .await?;
    let source = match r.player {
        JellyfinPlayer::Shim => Source::Jellyfin,
        JellyfinPlayer::Kodi => Source::Kodi,
    };
    Ok((StatusCode::ACCEPTED, Json(Started { source })))
}

#[derive(Deserialize, ToSchema)]
pub struct UrlRequest {
    /// A media address. A YouTube address plays in VacuumTube, a stream of
    /// the Jellyfin server of the shim in the shim, all others in mpv.
    url: String,
    /// The title for mpv
    title: Option<String>,
}

/// Plays a media address in the player that fits it.
#[utoipa::path(post, path = "/api/v1/play/url", tag = "play", request_body = UrlRequest,
    responses((status = 202, body = Started), (status = 400, body = ErrorBody)))]
async fn play_url(
    State(c): State<Arc<Controller>>,
    Json(r): Json<UrlRequest>,
) -> Result<(StatusCode, Json<Started>)> {
    let source = c.play_url(&r.url, r.title.as_deref()).await?;
    Ok((StatusCode::ACCEPTED, Json(Started { source })))
}

/// Pauses the player that plays.
#[utoipa::path(post, path = "/api/v1/playback/pause", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn pause(State(c): State<Arc<Controller>>) -> Result<StatusCode> {
    c.set_paused(Some(true))
        .await
        .map(|_| StatusCode::NO_CONTENT)
}

/// Continues the paused media.
#[utoipa::path(post, path = "/api/v1/playback/resume", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn resume(State(c): State<Arc<Controller>>) -> Result<StatusCode> {
    c.set_paused(Some(false))
        .await
        .map(|_| StatusCode::NO_CONTENT)
}

/// Pauses or continues.
#[utoipa::path(post, path = "/api/v1/playback/toggle", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn toggle(State(c): State<Arc<Controller>>) -> Result<StatusCode> {
    c.set_paused(None).await.map(|_| StatusCode::NO_CONTENT)
}

/// Stops the media. Kodi and VacuumTube keep running.
#[utoipa::path(post, path = "/api/v1/playback/stop", tag = "playback",
    responses((status = 204), (status = 409, body = ErrorBody)))]
async fn stop(State(c): State<Arc<Controller>>) -> Result<StatusCode> {
    c.stop().await.map(|_| StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct SeekRequest {
    /// Seconds from the start
    position: f64,
}

/// Goes to a position in the media.
#[utoipa::path(post, path = "/api/v1/playback/seek", tag = "playback", request_body = SeekRequest,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
async fn seek(State(c): State<Arc<Controller>>, Json(r): Json<SeekRequest>) -> Result<StatusCode> {
    c.seek(r.position).await.map(|_| StatusCode::NO_CONTENT)
}

/// Stops all media and ends Kodi and VacuumTube, so the home screen shows.
#[utoipa::path(post, path = "/api/v1/home", tag = "device", responses((status = 204)))]
async fn home(State(c): State<Arc<Controller>>) -> Result<StatusCode> {
    c.home().await.map(|_| StatusCode::NO_CONTENT)
}

/// Sends Wake-on-LAN to the TV, with the addresses of WAKE_ON_LAN_MACS.
#[utoipa::path(post, path = "/api/v1/tv/wake", tag = "device",
    responses((status = 204), (status = 503, body = ErrorBody)))]
async fn wake_tv(State(c): State<Arc<Controller>>) -> Result<StatusCode> {
    c.wake_tv().await.map(|_| StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_all_paths() {
        let doc = ApiDoc::openapi();
        for path in [
            "/health",
            "/api/v1/status",
            "/api/v1/play/youtube",
            "/api/v1/play/jellyfin",
            "/api/v1/apps/{id}/launch",
        ] {
            assert!(doc.paths.paths.contains_key(path), "{path} is missing");
        }
    }
}
