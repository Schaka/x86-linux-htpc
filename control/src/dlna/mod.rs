// SPDX-License-Identifier: AGPL-3.0-only
//! A UPnP AV media renderer (DLNA DMR) on top of the controller.
//!
//! DLNA knows only "play this address". The renderer gives the address to
//! Controller::play_url, which picks the player. The state that the
//! renderer reports is the state of the player that plays, also for media
//! that another app started, for example a cast from a phone. Home
//! Assistant finds the renderer by SSDP and makes a media player entity
//! for it.

mod description;
pub mod ssdp;
mod xml;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use tokio::sync::Notify;

use crate::control::Controller;
use crate::error::ApiError;
use crate::status::{PlayState, Source, Status};
use description::{AV_TRANSPORT, CONNECTION_MANAGER, RENDERING_CONTROL, SINK_PROTOCOLS};
use xml::{element_text, escape};

/// A start through DLNA shows TRANSITIONING for at most this long, until
/// the player reports the media.
const START_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_SUBSCRIBERS: usize = 32;
const DEFAULT_SUBSCRIPTION: u64 = 1800;
/// After SetAVTransportURI, the renderer reports STOPPED for at most this
/// long, also while another player plays. A control point calls Play only
/// on a renderer that does not play, and Home Assistant reads the state
/// before the player had time to stop.
const SET_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Dlna {
    controller: Arc<Controller>,
    pub udn: String,
    pub name: String,
    http: reqwest::Client,
    renderer: Arc<Mutex<Renderer>>,
    subscribers: Mutex<HashMap<String, Subscriber>>,
    changed: Notify,
}

/// What the control point set with SetAVTransportURI
#[derive(Clone, Default)]
struct Renderer {
    uri: String,
    metadata: String,
    /// The renderer started the address, and it still plays
    started: bool,
    starting_since: Option<Instant>,
    /// The time of SetAVTransportURI, until Play or Stop
    set_since: Option<Instant>,
}

struct Subscriber {
    service: &'static str,
    callback: String,
    expires: Instant,
    seq: u32,
}

impl Dlna {
    pub fn new(controller: Arc<Controller>) -> Arc<Dlna> {
        Arc::new(Dlna {
            udn: format!("uuid:{}", stable_uuid()),
            name: controller.config.name.clone(),
            controller,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("the HTTP client starts"),
            renderer: Arc::new(Mutex::new(Renderer::default())),
            subscribers: Mutex::new(HashMap::new()),
            changed: Notify::new(),
        })
    }

    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route(
                "/dlna/device.xml",
                get(|State(d): State<Arc<Dlna>>| async move {
                    xml_response(description::device(&d.name, &d.udn))
                }),
            )
            .route(
                "/dlna/AVTransport.xml",
                get(|| async { xml_response(description::av_transport()) }),
            )
            .route(
                "/dlna/RenderingControl.xml",
                get(|| async { xml_response(description::rendering_control()) }),
            )
            .route(
                "/dlna/ConnectionManager.xml",
                get(|| async { xml_response(description::connection_manager()) }),
            )
            .route(
                "/dlna/AVTransport/control",
                post(|s, h, b| control(s, AV_TRANSPORT, h, b)),
            )
            .route(
                "/dlna/RenderingControl/control",
                post(|s, h, b| control(s, RENDERING_CONTROL, h, b)),
            )
            .route(
                "/dlna/ConnectionManager/control",
                post(|s, h, b| control(s, CONNECTION_MANAGER, h, b)),
            )
            .route(
                "/dlna/AVTransport/event",
                any(|s, r| subscription(s, AV_TRANSPORT, r)),
            )
            .route(
                "/dlna/RenderingControl/event",
                any(|s, r| subscription(s, RENDERING_CONTROL, r)),
            )
            .route(
                "/dlna/ConnectionManager/event",
                any(|s, r| subscription(s, CONNECTION_MANAGER, r)),
            )
            .with_state(self.clone())
    }

    fn renderer(&self) -> Renderer {
        self.renderer.lock().unwrap().clone()
    }

    /// Sends an event to the subscribers of AVTransport when the media or
    /// the state changes. The position changes each second, and control
    /// points ask for it with GetPositionInfo.
    pub async fn send_events_forever(self: Arc<Self>) {
        let mut updates = self.controller.subscribe();
        let mut last: Option<Status> = None;
        loop {
            tokio::select! {
                result = updates.changed() => if result.is_err() { return },
                _ = self.changed.notified() => last = None,
            }
            let status = updates.borrow_and_update().clone();
            self.follow_player(&status);
            if last
                .as_ref()
                .is_some_and(|l| l.same_media_and_state(&status))
            {
                continue;
            }
            last = Some(status.clone());
            self.notify(
                AV_TRANSPORT,
                vec![("LastChange".into(), self.av_last_change(&status))],
            );
        }
    }

    /// Keeps the renderer state in line with the player.
    fn follow_player(&self, status: &Status) {
        let mut renderer = self.renderer.lock().unwrap();
        if status.source != Source::Idle {
            renderer.starting_since = None;
        } else if renderer
            .starting_since
            .is_none_or(|since| since.elapsed() > START_TIMEOUT)
        {
            renderer.starting_since = None;
            renderer.started = false;
        }
    }

    fn transport_state(&self, status: &Status, renderer: &Renderer) -> &'static str {
        match status.state {
            _ if renderer
                .set_since
                .is_some_and(|since| since.elapsed() < SET_TIMEOUT) =>
            {
                "STOPPED"
            }
            PlayState::Playing => "PLAYING",
            PlayState::Paused => "PAUSED_PLAYBACK",
            PlayState::Buffering => "TRANSITIONING",
            PlayState::Idle if renderer.starting_since.is_some() => "TRANSITIONING",
            PlayState::Idle if !renderer.uri.is_empty() => "STOPPED",
            PlayState::Idle => "NO_MEDIA_PRESENT",
        }
    }

    fn transport_actions(state: &str) -> &'static str {
        match state {
            "PLAYING" | "TRANSITIONING" => "Pause,Stop,Seek",
            "PAUSED_PLAYBACK" => "Play,Stop,Seek",
            "STOPPED" => "Play",
            _ => "",
        }
    }

    /// The address and metadata of the media that plays. Media that the
    /// renderer did not start gets metadata from the player.
    fn current_media(&self, status: &Status, renderer: &Renderer) -> (String, String) {
        if status.source == Source::Idle || renderer.set_since.is_some() {
            return (renderer.uri.clone(), renderer.metadata.clone());
        }
        let uri = if renderer.started {
            renderer.uri.clone()
        } else {
            status.url.clone().unwrap_or_default()
        };
        (uri.clone(), didl(status, &uri))
    }

    fn av_last_change(&self, status: &Status) -> String {
        let renderer = self.renderer();
        let state = self.transport_state(status, &renderer);
        let (uri, metadata) = self.current_media(status, &renderer);
        let duration = hms(status.duration.unwrap_or(0.0));
        let values = [
            ("TransportState", state.to_string()),
            ("TransportStatus", "OK".into()),
            (
                "CurrentTransportActions",
                Self::transport_actions(state).into(),
            ),
            ("AVTransportURI", uri.clone()),
            ("AVTransportURIMetaData", metadata.clone()),
            ("CurrentTrackURI", uri),
            ("CurrentTrackMetaData", metadata),
            ("CurrentMediaDuration", duration.clone()),
            ("CurrentTrackDuration", duration),
            (
                "NumberOfTracks",
                if state == "NO_MEDIA_PRESENT" {
                    "0"
                } else {
                    "1"
                }
                .into(),
            ),
            ("CurrentPlayMode", "NORMAL".into()),
            ("TransportPlaySpeed", "1".into()),
        ];
        last_change("urn:schemas-upnp-org:metadata-1-0/AVT/", &values)
    }

    fn initial_event(&self, service: &str) -> Vec<(String, String)> {
        match service {
            AV_TRANSPORT => vec![(
                "LastChange".into(),
                self.av_last_change(&self.controller.status()),
            )],
            RENDERING_CONTROL => vec![(
                "LastChange".into(),
                // The renderer has no rendering values. An InstanceID
                // without values makes control points warn, so the event
                // is empty.
                String::new(),
            )],
            _ => vec![
                ("SourceProtocolInfo".into(), String::new()),
                ("SinkProtocolInfo".into(), SINK_PROTOCOLS.into()),
                ("CurrentConnectionIDs".into(), "0".into()),
            ],
        }
    }

    /// Sends an event to all subscribers of a service, or to one SID.
    fn notify(&self, service: &'static str, properties: Vec<(String, String)>) {
        self.notify_where(properties, |sid, s| s.service == service && !sid.is_empty());
    }

    fn notify_where(
        &self,
        properties: Vec<(String, String)>,
        select: impl Fn(&str, &Subscriber) -> bool,
    ) {
        let body = property_set(&properties);
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|_, s| s.expires > Instant::now());
        for (sid, subscriber) in subscribers.iter_mut().filter(|(sid, s)| select(sid, s)) {
            let request = self
                .http
                .request(
                    reqwest::Method::from_bytes(b"NOTIFY").expect("NOTIFY is a method"),
                    &subscriber.callback,
                )
                .header("Content-Type", "text/xml; charset=\"utf-8\"")
                .header("NT", "upnp:event")
                .header("NTS", "upnp:propchange")
                .header("SID", sid.as_str())
                .header("SEQ", subscriber.seq.to_string())
                .body(body.clone());
            subscriber.seq = subscriber.seq.wrapping_add(1).max(1);
            let callback = subscriber.callback.clone();
            tokio::spawn(async move {
                if let Err(error) = request.send().await {
                    tracing::debug!("DLNA event to {callback} failed: {error}");
                }
            });
        }
    }

    async fn av_transport(
        &self,
        action: &str,
        body: &str,
    ) -> Result<Vec<(&'static str, String)>, Fault> {
        let status = self.controller.status();
        let renderer = self.renderer();
        let state = self.transport_state(&status, &renderer);
        let (uri, metadata) = self.current_media(&status, &renderer);
        match action {
            "SetAVTransportURI" => {
                let uri = element_text(body, "CurrentURI").unwrap_or_default();
                let metadata = element_text(body, "CurrentURIMetaData").unwrap_or_default();
                tracing::info!("DLNA address {uri}");
                *self.renderer.lock().unwrap() = Renderer {
                    uri,
                    metadata,
                    started: false,
                    starting_since: None,
                    set_since: Some(Instant::now()),
                };
                self.changed.notify_one();
                Ok(vec![])
            }
            "Play" => {
                if !renderer.uri.is_empty() && !renderer.started {
                    self.start(renderer);
                } else {
                    self.controller
                        .set_paused(Some(false))
                        .await
                        .map_err(Fault::from)?;
                }
                Ok(vec![])
            }
            "Pause" => self
                .controller
                .set_paused(Some(true))
                .await
                .map(|_| vec![])
                .map_err(Fault::from),
            "Stop" => {
                {
                    let mut current = self.renderer.lock().unwrap();
                    current.started = false;
                    current.set_since = None;
                }
                match self.controller.stop().await {
                    Ok(()) => {}
                    Err(e) if e.code == "nothing_plays" => {}
                    Err(e) => return Err(e.into()),
                }
                self.changed.notify_one();
                Ok(vec![])
            }
            "Seek" => {
                let unit = element_text(body, "Unit").unwrap_or_default();
                let target = element_text(body, "Target").unwrap_or_default();
                if unit != "REL_TIME" && unit != "ABS_TIME" {
                    return Err(Fault::new(710, "Seek mode not supported"));
                }
                let position = parse_hms(&target).ok_or(Fault::new(711, "Illegal seek target"))?;
                self.controller
                    .seek(position)
                    .await
                    .map(|_| vec![])
                    .map_err(Fault::from)
            }
            "GetTransportInfo" => Ok(vec![
                ("CurrentTransportState", state.into()),
                ("CurrentTransportStatus", "OK".into()),
                ("CurrentSpeed", "1".into()),
            ]),
            "GetPositionInfo" => {
                let position = hms(status.position.unwrap_or(0.0));
                Ok(vec![
                    ("Track", if uri.is_empty() { "0" } else { "1" }.into()),
                    ("TrackDuration", hms(status.duration.unwrap_or(0.0))),
                    ("TrackMetaData", metadata),
                    ("TrackURI", uri),
                    ("RelTime", position.clone()),
                    ("AbsTime", position),
                    ("RelCount", "2147483647".into()),
                    ("AbsCount", "2147483647".into()),
                ])
            }
            "GetMediaInfo" => Ok(vec![
                ("NrTracks", if uri.is_empty() { "0" } else { "1" }.into()),
                ("MediaDuration", hms(status.duration.unwrap_or(0.0))),
                ("CurrentURI", uri),
                ("CurrentURIMetaData", metadata),
                ("NextURI", String::new()),
                ("NextURIMetaData", String::new()),
                (
                    "PlayMedium",
                    if state == "NO_MEDIA_PRESENT" {
                        "NONE"
                    } else {
                        "NETWORK"
                    }
                    .into(),
                ),
                ("RecordMedium", "NOT_IMPLEMENTED".into()),
                ("WriteStatus", "NOT_IMPLEMENTED".into()),
            ]),
            "GetTransportSettings" => Ok(vec![
                ("PlayMode", "NORMAL".into()),
                ("RecQualityMode", "NOT_IMPLEMENTED".into()),
            ]),
            "GetCurrentTransportActions" => {
                Ok(vec![("Actions", Self::transport_actions(state).into())])
            }
            "GetDeviceCapabilities" => Ok(vec![
                ("PlayMedia", "NETWORK".into()),
                ("RecMedia", "NOT_IMPLEMENTED".into()),
                ("RecQualityModes", "NOT_IMPLEMENTED".into()),
            ]),
            _ => Err(Fault::new(401, "Invalid Action")),
        }
    }

    /// The start can take some seconds, for example when Kodi must end
    /// first. The answer to Play goes out at once, and the state shows
    /// TRANSITIONING until the player has the media.
    fn start(&self, renderer: Renderer) {
        {
            let mut current = self.renderer.lock().unwrap();
            current.started = true;
            current.starting_since = Some(Instant::now());
            current.set_since = None;
        }
        self.changed.notify_one();
        let controller = self.controller.clone();
        let title = element_text(&renderer.metadata, "title");
        let shared = self.renderer.clone();
        tokio::spawn(async move {
            if let Err(error) = controller.play_url(&renderer.uri, title.as_deref()).await {
                tracing::warn!("DLNA start of {} failed: {error}", renderer.uri);
                let mut current = shared.lock().unwrap();
                current.started = false;
                current.starting_since = None;
            }
        });
    }

    async fn connection_manager(&self, action: &str) -> Result<Vec<(&'static str, String)>, Fault> {
        match action {
            "GetProtocolInfo" => Ok(vec![
                ("Source", String::new()),
                ("Sink", SINK_PROTOCOLS.into()),
            ]),
            "GetCurrentConnectionIDs" => Ok(vec![("ConnectionIDs", "0".into())]),
            "GetCurrentConnectionInfo" => Ok(vec![
                ("RcsID", "0".into()),
                ("AVTransportID", "0".into()),
                ("ProtocolInfo", String::new()),
                ("PeerConnectionManager", String::new()),
                ("PeerConnectionID", "-1".into()),
                ("Direction", "Input".into()),
                ("Status", "OK".into()),
            ]),
            _ => Err(Fault::new(401, "Invalid Action")),
        }
    }
}

/// A UPnP error answer
struct Fault {
    code: u16,
    description: &'static str,
}

impl Fault {
    fn new(code: u16, description: &'static str) -> Fault {
        Fault { code, description }
    }
}

impl From<ApiError> for Fault {
    fn from(error: ApiError) -> Fault {
        tracing::info!("DLNA action failed: {error}");
        match error.code {
            "nothing_plays" => Fault::new(701, "Transition not available"),
            "bad_request" => Fault::new(402, "Invalid Args"),
            _ => Fault::new(501, "Action Failed"),
        }
    }
}

async fn control(
    State(dlna): State<Arc<Dlna>>,
    service: &'static str,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let body = String::from_utf8_lossy(&body);
    let action = headers
        .get("soapaction")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_matches('"'))
        .and_then(|v| v.rsplit_once('#'))
        .map(|(_, a)| a.to_string())
        .unwrap_or_default();
    let result = match service {
        AV_TRANSPORT => dlna.av_transport(&action, &body).await,
        RENDERING_CONTROL => Err(Fault::new(401, "Invalid Action")),
        _ => dlna.connection_manager(&action).await,
    };
    match result {
        Ok(values) => {
            let arguments: String = values
                .iter()
                .map(|(k, v)| format!("<{k}>{}</{k}>", escape(v)))
                .collect();
            let body = format!(
                "<u:{action}Response xmlns:u=\"{service}\">{arguments}</u:{action}Response>"
            );
            xml_response(envelope(&body))
        }
        Err(fault) => {
            let body = format!(
                "<s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail>\
                 <UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{}</errorCode>\
                 <errorDescription>{}</errorDescription></UPnPError></detail></s:Fault>",
                fault.code, fault.description
            );
            let mut response = xml_response(envelope(&body));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response
        }
    }
}

/// SUBSCRIBE and UNSUBSCRIBE of the UPnP event protocol (GENA)
async fn subscription(
    State(dlna): State<Arc<Dlna>>,
    service: &'static str,
    request: Request,
) -> Response {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let seconds = header("timeout")
        .and_then(|t| {
            t.strip_prefix("Second-")
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(DEFAULT_SUBSCRIPTION)
        .clamp(60, 3600);
    match request.method().as_str() {
        "SUBSCRIBE" => {
            if let Some(sid) = header("sid") {
                let mut subscribers = dlna.subscribers.lock().unwrap();
                return match subscribers.get_mut(&sid) {
                    Some(s) => {
                        s.expires = Instant::now() + Duration::from_secs(seconds);
                        subscribed(&sid, seconds)
                    }
                    None => StatusCode::PRECONDITION_FAILED.into_response(),
                };
            }
            let callback = header("callback")
                .and_then(|c| {
                    c.split('>')
                        .next()
                        .map(|c| c.trim_start_matches('<').to_string())
                })
                .filter(|c| c.starts_with("http://"));
            let (Some(callback), Some("upnp:event")) = (callback, header("nt").as_deref()) else {
                return StatusCode::PRECONDITION_FAILED.into_response();
            };
            let sid = format!("uuid:{}", random_uuid());
            {
                let mut subscribers = dlna.subscribers.lock().unwrap();
                subscribers.retain(|_, s| s.expires > Instant::now());
                if subscribers.len() >= MAX_SUBSCRIBERS {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                subscribers.insert(
                    sid.clone(),
                    Subscriber {
                        service,
                        callback,
                        expires: Instant::now() + Duration::from_secs(seconds),
                        seq: 0,
                    },
                );
            }
            // The first event follows the answer, with all evented values.
            let first = dlna.clone();
            let first_sid = sid.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let properties = first.initial_event(service);
                first.notify_where(properties, |s, _| s == first_sid);
            });
            subscribed(&sid, seconds)
        }
        "UNSUBSCRIBE" => {
            let removed =
                header("sid").and_then(|sid| dlna.subscribers.lock().unwrap().remove(&sid));
            if removed.is_some() {
                StatusCode::OK
            } else {
                StatusCode::PRECONDITION_FAILED
            }
            .into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

fn subscribed(sid: &str, seconds: u64) -> Response {
    let mut response = StatusCode::OK.into_response();
    let headers = response.headers_mut();
    headers.insert("SID", HeaderValue::from_str(sid).expect("the SID is ASCII"));
    headers.insert(
        "TIMEOUT",
        HeaderValue::from_str(&format!("Second-{seconds}")).expect("ASCII"),
    );
    response
}

fn xml_response(body: String) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/xml; charset=\"utf-8\""),
    );
    response
}

fn envelope(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>{body}</s:Body></s:Envelope>"
    )
}

/// The LastChange value: an XML document as text, so it is escaped once
/// here and once more in the property set.
fn last_change(namespace: &str, values: &[(&str, String)]) -> String {
    let fields: String = values
        .iter()
        .map(|(k, v)| format!("<{k} val=\"{}\"/>", escape(v)))
        .collect();
    format!("<Event xmlns=\"{namespace}\"><InstanceID val=\"0\">{fields}</InstanceID></Event>")
}

fn property_set(properties: &[(String, String)]) -> String {
    let fields: String = properties
        .iter()
        .map(|(k, v)| format!("<e:property><{k}>{}</{k}></e:property>", escape(v)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\">{fields}</e:propertyset>"
    )
}

/// DIDL-Lite metadata of the media that plays
fn didl(status: &Status, uri: &str) -> String {
    let mut item = String::new();
    if let Some(title) = &status.title {
        item.push_str(&format!("<dc:title>{}</dc:title>", escape(title)));
    }
    if let Some(subtitle) = &status.subtitle {
        item.push_str(&format!(
            "<dc:creator>{0}</dc:creator><upnp:artist>{0}</upnp:artist>",
            escape(subtitle)
        ));
    }
    if let Some(image) = &status.image {
        item.push_str(&format!(
            "<upnp:albumArtURI>{}</upnp:albumArtURI>",
            escape(image)
        ));
    }
    item.push_str("<upnp:class>object.item.videoItem</upnp:class>");
    let duration = status
        .duration
        .map(|d| format!(" duration=\"{}\"", hms(d)))
        .unwrap_or_default();
    item.push_str(&format!(
        "<res protocolInfo=\"http-get:*:*:*\"{duration}>{}</res>",
        escape(uri)
    ));
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"0\" parentID=\"-1\" restricted=\"1\">{item}</item></DIDL-Lite>"
    )
}

/// H:MM:SS, the time format of UPnP
fn hms(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
}

/// H:MM:SS with an optional fraction, or MM:SS
fn parse_hms(text: &str) -> Option<f64> {
    let parts: Vec<f64> = text
        .trim()
        .split(':')
        .map(|p| p.parse::<f64>().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [h, m, s] => Some(h * 3600.0 + m * 60.0 + s),
        [m, s] => Some(m * 60.0 + s),
        [s] => Some(*s),
        _ => None,
    }
}

fn format_uuid(bytes: [u8; 16]) -> String {
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn random_uuid() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        bytes = now.to_le_bytes();
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(bytes)
}

/// The UDN must stay the same over restarts, or Home Assistant sees a new
/// device. It comes from a hash of the machine ID, so the machine ID itself
/// does not go out on the network.
fn stable_uuid() -> String {
    let machine_id =
        std::fs::read_to_string("/etc/machine-id").unwrap_or_else(|_| "x86-linux-htpc".into());
    let hash = |salt: &str| {
        // FNV-1a, 64 bit
        let mut h: u64 = 0xcbf29ce484222325;
        for b in salt.bytes().chain(machine_id.trim().bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    };
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hash("htpc-control-dlna-a").to_be_bytes());
    bytes[8..].copy_from_slice(&hash("htpc-control-dlna-b").to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_times() {
        assert_eq!(hms(3725.9), "1:02:05");
        assert_eq!(parse_hms("1:02:05.500"), Some(3725.5));
        assert_eq!(parse_hms("02:05"), Some(125.0));
        assert_eq!(parse_hms("x"), None);
    }

    #[test]
    fn escapes_last_change_twice() {
        let change = last_change("urn:x", &[("AVTransportURI", "http://h/a?b=1&c=2".into())]);
        assert!(change.contains("val=\"http://h/a?b=1&amp;c=2\""));
        let set = property_set(&[("LastChange".into(), change)]);
        assert!(set.contains("&lt;Event") && set.contains("&amp;amp;c=2"));
    }

    #[test]
    fn keeps_the_udn() {
        assert_eq!(stable_uuid(), stable_uuid());
        assert_ne!(random_uuid(), random_uuid());
    }
}
