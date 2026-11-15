// SPDX-License-Identifier: AGPL-3.0-only
//! Tests of whole casts. A phone sends the requests that a DLNA app or the
//! REST API sends, and the tests read the command that would start mpv.
//! Only the start of the program is replaced.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use reqwest::StatusCode;

use crate::config::Config;
use crate::control::Controller;
use crate::dlna::Dlna;
use crate::dlna::xml::escape;
use crate::players::mpv::{Mpv, Spawner};

/// The commands that the service started, one list of arguments for each
type Started = Arc<Mutex<Vec<Vec<String>>>>;

struct Htpc {
    api: String,
    home: std::path::PathBuf,
    started: Started,
}

impl Htpc {
    /// A service with the REST API and the DLNA renderer, on a free port,
    /// that starts no program.
    async fn start(name: &str) -> Htpc {
        let home =
            std::env::temp_dir().join(format!("htpc-control-flow-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let config = Config::for_test(home.clone());
        let started = Started::default();
        let record = started.clone();
        let spawner: Spawner = Arc::new(move |argv| {
            record.lock().unwrap().push(argv);
            Box::pin(async { Ok(()) })
        });
        let controller = Controller::with_mpv(config.clone(), Mpv::with_spawner(&config, spawner));
        let dlna = Dlna::new(controller.clone());
        let router: Router = crate::api::router(controller).merge(dlna.router());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await });
        Htpc { api, home, started }
    }

    /// The arguments of the first program that the service started, after
    /// at most five seconds
    async fn first_start(&self) -> Option<Vec<String>> {
        for _ in 0..50 {
            if let Some(argv) = self.started.lock().unwrap().first().cloned() {
                return Some(argv);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        None
    }

    /// An action of the AVTransport service, as a DLNA app sends it
    async fn soap(&self, action: &str, arguments: &str) -> StatusCode {
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>\
             <u:{action} xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
             <InstanceID>0</InstanceID>{arguments}</u:{action}></s:Body></s:Envelope>"
        );
        reqwest::Client::new()
            .post(format!("{}/dlna/AVTransport/control", self.api))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header(
                "SOAPACTION",
                format!("\"urn:schemas-upnp-org:service:AVTransport:1#{action}\""),
            )
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    /// A cast from a DLNA app: the address with its metadata, then Play
    async fn dlna_cast(&self, address: &str, metadata: &str) {
        let arguments = format!(
            "<CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            escape(address),
            escape(metadata)
        );
        assert_eq!(
            self.soap("SetAVTransportURI", &arguments).await,
            StatusCode::OK
        );
        assert_eq!(self.soap("Play", "<Speed>1</Speed>").await, StatusCode::OK);
    }

    async fn play_url(&self, body: serde_json::Value) -> StatusCode {
        reqwest::Client::new()
            .post(format!("{}/api/v1/play/url", self.api))
            .json(&body)
            .send()
            .await
            .unwrap()
            .status()
    }
}

impl Drop for Htpc {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn didl(class: &str, title: &str) -> String {
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">\
         <item id=\"1\" parentID=\"0\" restricted=\"1\"><dc:title>{title}</dc:title>\
         <upnp:class>{class}</upnp:class></item></DIDL-Lite>"
    )
}

fn has(argv: &[String], argument: &str) -> bool {
    argv.iter().any(|a| a == argument)
}

fn plays_without_picture(argv: &[String]) -> bool {
    has(argv, "--no-video") && !has(argv, "--fullscreen")
}

#[tokio::test]
async fn music_from_a_dlna_app_plays_without_picture() {
    let htpc = Htpc::start("dlna-music").await;
    // An address without a file extension, as a streaming server sends it
    htpc.dlna_cast(
        "http://media.example/stream/42",
        &didl("object.item.audioItem.musicTrack", "Song"),
    )
    .await;
    let argv = htpc.first_start().await.expect("a start of mpv");
    assert!(plays_without_picture(&argv), "{argv:?}");
    assert!(has(&argv, "--force-media-title=Song"), "{argv:?}");
    assert_eq!(argv.last().unwrap(), "http://media.example/stream/42");
    assert_eq!(htpc.started.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_video_from_a_dlna_app_plays_with_the_picture() {
    let htpc = Htpc::start("dlna-video").await;
    htpc.dlna_cast(
        "http://media.example/stream/43",
        &didl("object.item.videoItem.movie", "Film"),
    )
    .await;
    let argv = htpc.first_start().await.expect("a start of mpv");
    assert!(has(&argv, "--fullscreen"), "{argv:?}");
    assert!(!has(&argv, "--no-video"), "{argv:?}");
    assert!(
        argv.iter().any(|a| a.starts_with("--config-dir=")),
        "{argv:?}"
    );
}

#[tokio::test]
async fn an_mp3_file_from_a_dlna_app_without_metadata_plays_without_picture() {
    let htpc = Htpc::start("dlna-mp3").await;
    htpc.dlna_cast("http://media.example/Music/Track.MP3?token=1", "")
        .await;
    let argv = htpc.first_start().await.expect("a start of mpv");
    assert!(plays_without_picture(&argv), "{argv:?}");
}

#[tokio::test]
async fn play_without_an_address_starts_nothing() {
    let htpc = Htpc::start("dlna-empty").await;
    // A renderer without media answers a UPnP fault to Play.
    let status = htpc.soap("Play", "<Speed>1</Speed>").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(htpc.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_rest_api_plays_audio_on_request_and_by_extension() {
    let htpc = Htpc::start("rest-audio").await;
    let status = htpc
        .play_url(serde_json::json!({"url": "http://media.example/live", "audio": true, "title": "Radio"}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let argv = htpc.first_start().await.expect("a start of mpv");
    assert!(plays_without_picture(&argv), "{argv:?}");
    assert!(has(&argv, "--force-media-title=Radio"), "{argv:?}");

    let by_extension = Htpc::start("rest-extension").await;
    let status = by_extension
        .play_url(serde_json::json!({"url": "http://media.example/a.flac"}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let argv = by_extension.first_start().await.expect("a start of mpv");
    assert!(plays_without_picture(&argv), "{argv:?}");
}

#[tokio::test]
async fn the_rest_api_plays_other_addresses_with_the_picture() {
    let htpc = Htpc::start("rest-video").await;
    let status = htpc
        .play_url(serde_json::json!({"url": "http://media.example/film.mkv"}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let argv = htpc.first_start().await.expect("a start of mpv");
    assert!(has(&argv, "--fullscreen"), "{argv:?}");
    assert!(!has(&argv, "--no-video"), "{argv:?}");
}

#[tokio::test]
async fn the_rest_api_refuses_addresses_that_are_not_media_addresses() {
    let htpc = Htpc::start("rest-refuse").await;
    for url in [
        "file:///etc/passwd",
        "ftp://media.example/a.mp3",
        "not an address",
    ] {
        let status = htpc
            .play_url(serde_json::json!({"url": url, "audio": true}))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{url}");
    }
    assert!(htpc.started.lock().unwrap().is_empty());
}
