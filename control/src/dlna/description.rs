// SPDX-License-Identifier: AGPL-3.0-only
//! The device description and the service descriptions (SCPD) of the
//! renderer. They list only the actions that the renderer has. Home
//! Assistant shows a control, for example the volume, only when the
//! service description has its action.

use super::xml::escape;

pub const DEVICE_TYPE: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
pub const AV_TRANSPORT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
pub const RENDERING_CONTROL: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
pub const CONNECTION_MANAGER: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";

/// The formats that the players take. mpv plays nearly all formats, so the
/// list starts with the wildcard, and names the common formats for control
/// points that look for a match.
pub const SINK_PROTOCOLS: &str = "http-get:*:*:*,http-get:*:video/mp4:*,http-get:*:video/x-matroska:*,\
http-get:*:video/webm:*,http-get:*:video/mpeg:*,http-get:*:video/mp2t:*,http-get:*:video/quicktime:*,\
http-get:*:application/vnd.apple.mpegurl:*,http-get:*:audio/mpeg:*,http-get:*:audio/flac:*,\
http-get:*:audio/mp4:*,http-get:*:audio/ogg:*,http-get:*:audio/wav:*";

/// The short name of a service in the paths, and its type
pub const SERVICES: [(&str, &str); 3] = [
    ("AVTransport", AV_TRANSPORT),
    ("RenderingControl", RENDERING_CONTROL),
    ("ConnectionManager", CONNECTION_MANAGER),
];

pub fn device(name: &str, udn: &str) -> String {
    let services: String = SERVICES
        .iter()
        .map(|(short, kind)| {
            format!(
                "<service><serviceType>{kind}</serviceType><serviceId>urn:upnp-org:serviceId:{short}</serviceId>\
                 <SCPDURL>/dlna/{short}.xml</SCPDURL><controlURL>/dlna/{short}/control</controlURL>\
                 <eventSubURL>/dlna/{short}/event</eventSubURL></service>"
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0" xmlns:dlna="urn:schemas-dlna-org:device-1-0">
<specVersion><major>1</major><minor>0</minor></specVersion>
<device>
<deviceType>{DEVICE_TYPE}</deviceType>
<friendlyName>{name}</friendlyName>
<manufacturer>x86-linux-htpc</manufacturer>
<manufacturerURL>https://github.com/Schaka/x86-linux-htpc</manufacturerURL>
<modelDescription>HTPC with jellyfin-mpv-shim, VacuumTube and Kodi</modelDescription>
<modelName>x86-linux-htpc</modelName>
<modelNumber>{version}</modelNumber>
<UDN>{udn}</UDN>
<dlna:X_DLNADOC>DMR-1.50</dlna:X_DLNADOC>
<serviceList>{services}</serviceList>
</device>
</root>
"#,
        name = escape(name),
        version = env!("CARGO_PKG_VERSION"),
    )
}

struct Variable {
    name: &'static str,
    kind: &'static str,
    events: bool,
    allowed: &'static [&'static str],
}

const fn var(name: &'static str, kind: &'static str) -> Variable {
    Variable {
        name,
        kind,
        events: false,
        allowed: &[],
    }
}

const fn allowed(name: &'static str, values: &'static [&'static str]) -> Variable {
    Variable {
        name,
        kind: "string",
        events: false,
        allowed: values,
    }
}

const fn evented(name: &'static str) -> Variable {
    Variable {
        name,
        kind: "string",
        events: true,
        allowed: &[],
    }
}

/// An action: its name, and its arguments as (name, out, state variable)
type Action = (&'static str, &'static [(&'static str, bool, &'static str)]);

fn scpd(actions: &[Action], variables: &[Variable]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0"><specVersion><major>1</major><minor>0</minor></specVersion><actionList>"#,
    );
    for (name, arguments) in actions {
        xml.push_str(&format!("<action><name>{name}</name><argumentList>"));
        for (argument, out, related) in *arguments {
            let direction = if *out { "out" } else { "in" };
            xml.push_str(&format!(
                "<argument><name>{argument}</name><direction>{direction}</direction>\
                 <relatedStateVariable>{related}</relatedStateVariable></argument>"
            ));
        }
        xml.push_str("</argumentList></action>");
    }
    xml.push_str("</actionList><serviceStateTable>");
    for v in variables {
        let events = if v.events { "yes" } else { "no" };
        xml.push_str(&format!(
            "<stateVariable sendEvents=\"{events}\"><name>{}</name><dataType>{}</dataType>",
            v.name, v.kind
        ));
        if !v.allowed.is_empty() {
            xml.push_str("<allowedValueList>");
            for value in v.allowed {
                xml.push_str(&format!("<allowedValue>{value}</allowedValue>"));
            }
            xml.push_str("</allowedValueList>");
        }
        xml.push_str("</stateVariable>");
    }
    xml.push_str("</serviceStateTable></scpd>\n");
    xml
}

const INSTANCE: (&str, bool, &str) = ("InstanceID", false, "A_ARG_TYPE_InstanceID");

pub fn av_transport() -> String {
    let actions: &[Action] = &[
        (
            "SetAVTransportURI",
            &[
                INSTANCE,
                ("CurrentURI", false, "AVTransportURI"),
                ("CurrentURIMetaData", false, "AVTransportURIMetaData"),
            ],
        ),
        (
            "GetMediaInfo",
            &[
                INSTANCE,
                ("NrTracks", true, "NumberOfTracks"),
                ("MediaDuration", true, "CurrentMediaDuration"),
                ("CurrentURI", true, "AVTransportURI"),
                ("CurrentURIMetaData", true, "AVTransportURIMetaData"),
                ("NextURI", true, "NextAVTransportURI"),
                ("NextURIMetaData", true, "NextAVTransportURIMetaData"),
                ("PlayMedium", true, "PlaybackStorageMedium"),
                ("RecordMedium", true, "RecordStorageMedium"),
                ("WriteStatus", true, "RecordMediumWriteStatus"),
            ],
        ),
        (
            "GetTransportInfo",
            &[
                INSTANCE,
                ("CurrentTransportState", true, "TransportState"),
                ("CurrentTransportStatus", true, "TransportStatus"),
                ("CurrentSpeed", true, "TransportPlaySpeed"),
            ],
        ),
        (
            "GetPositionInfo",
            &[
                INSTANCE,
                ("Track", true, "CurrentTrack"),
                ("TrackDuration", true, "CurrentTrackDuration"),
                ("TrackMetaData", true, "CurrentTrackMetaData"),
                ("TrackURI", true, "CurrentTrackURI"),
                ("RelTime", true, "RelativeTimePosition"),
                ("AbsTime", true, "AbsoluteTimePosition"),
                ("RelCount", true, "RelativeCounterPosition"),
                ("AbsCount", true, "AbsoluteCounterPosition"),
            ],
        ),
        (
            "GetDeviceCapabilities",
            &[
                INSTANCE,
                ("PlayMedia", true, "PossiblePlaybackStorageMedia"),
                ("RecMedia", true, "PossibleRecordStorageMedia"),
                ("RecQualityModes", true, "PossibleRecordQualityModes"),
            ],
        ),
        (
            "GetTransportSettings",
            &[
                INSTANCE,
                ("PlayMode", true, "CurrentPlayMode"),
                ("RecQualityMode", true, "CurrentRecordQualityMode"),
            ],
        ),
        (
            "GetCurrentTransportActions",
            &[INSTANCE, ("Actions", true, "CurrentTransportActions")],
        ),
        ("Stop", &[INSTANCE]),
        ("Play", &[INSTANCE, ("Speed", false, "TransportPlaySpeed")]),
        ("Pause", &[INSTANCE]),
        (
            "Seek",
            &[
                INSTANCE,
                ("Unit", false, "A_ARG_TYPE_SeekMode"),
                ("Target", false, "A_ARG_TYPE_SeekTarget"),
            ],
        ),
    ];
    let variables = [
        allowed(
            "TransportState",
            &[
                "STOPPED",
                "PLAYING",
                "PAUSED_PLAYBACK",
                "TRANSITIONING",
                "NO_MEDIA_PRESENT",
            ],
        ),
        allowed("TransportStatus", &["OK", "ERROR_OCCURRED"]),
        allowed("TransportPlaySpeed", &["1"]),
        allowed("PlaybackStorageMedium", &["NONE", "NETWORK"]),
        allowed("RecordStorageMedium", &["NOT_IMPLEMENTED"]),
        var("PossiblePlaybackStorageMedia", "string"),
        var("PossibleRecordStorageMedia", "string"),
        var("PossibleRecordQualityModes", "string"),
        allowed("RecordMediumWriteStatus", &["NOT_IMPLEMENTED"]),
        allowed("CurrentPlayMode", &["NORMAL"]),
        allowed("CurrentRecordQualityMode", &["NOT_IMPLEMENTED"]),
        var("NumberOfTracks", "ui4"),
        var("CurrentTrack", "ui4"),
        var("CurrentTrackDuration", "string"),
        var("CurrentMediaDuration", "string"),
        var("CurrentTrackMetaData", "string"),
        var("CurrentTrackURI", "string"),
        var("AVTransportURI", "string"),
        var("AVTransportURIMetaData", "string"),
        var("NextAVTransportURI", "string"),
        var("NextAVTransportURIMetaData", "string"),
        var("RelativeTimePosition", "string"),
        var("AbsoluteTimePosition", "string"),
        var("RelativeCounterPosition", "i4"),
        var("AbsoluteCounterPosition", "i4"),
        var("CurrentTransportActions", "string"),
        evented("LastChange"),
        allowed("A_ARG_TYPE_SeekMode", &["REL_TIME", "ABS_TIME", "TRACK_NR"]),
        var("A_ARG_TYPE_SeekTarget", "string"),
        var("A_ARG_TYPE_InstanceID", "ui4"),
    ];
    scpd(actions, &variables)
}

/// The service has no actions. Home Assistant needs the service for a
/// renderer, and shows a volume or sound mode control for each action that
/// it finds. With passthrough, the AV receiver has the volume, so the
/// renderer offers none.
pub fn rendering_control() -> String {
    let variables = [evented("LastChange"), var("A_ARG_TYPE_InstanceID", "ui4")];
    scpd(&[], &variables)
}

pub fn connection_manager() -> String {
    let actions: &[Action] = &[
        (
            "GetProtocolInfo",
            &[
                ("Source", true, "SourceProtocolInfo"),
                ("Sink", true, "SinkProtocolInfo"),
            ],
        ),
        (
            "GetCurrentConnectionIDs",
            &[("ConnectionIDs", true, "CurrentConnectionIDs")],
        ),
        (
            "GetCurrentConnectionInfo",
            &[
                ("ConnectionID", false, "A_ARG_TYPE_ConnectionID"),
                ("RcsID", true, "A_ARG_TYPE_RcsID"),
                ("AVTransportID", true, "A_ARG_TYPE_AVTransportID"),
                ("ProtocolInfo", true, "A_ARG_TYPE_ProtocolInfo"),
                (
                    "PeerConnectionManager",
                    true,
                    "A_ARG_TYPE_ConnectionManager",
                ),
                ("PeerConnectionID", true, "A_ARG_TYPE_ConnectionID"),
                ("Direction", true, "A_ARG_TYPE_Direction"),
                ("Status", true, "A_ARG_TYPE_ConnectionStatus"),
            ],
        ),
    ];
    let variables = [
        evented("SourceProtocolInfo"),
        evented("SinkProtocolInfo"),
        evented("CurrentConnectionIDs"),
        allowed(
            "A_ARG_TYPE_ConnectionStatus",
            &[
                "OK",
                "ContentFormatMismatch",
                "InsufficientBandwidth",
                "UnreliableChannel",
                "Unknown",
            ],
        ),
        var("A_ARG_TYPE_ConnectionManager", "string"),
        allowed("A_ARG_TYPE_Direction", &["Input", "Output"]),
        var("A_ARG_TYPE_ProtocolInfo", "string"),
        var("A_ARG_TYPE_ConnectionID", "i4"),
        var("A_ARG_TYPE_AVTransportID", "i4"),
        var("A_ARG_TYPE_RcsID", "i4"),
    ];
    scpd(actions, &variables)
}
