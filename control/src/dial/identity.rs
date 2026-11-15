// SPDX-License-Identifier: AGPL-3.0-only
//! The name and the UDN (unique device name) that a phone knows the device
//! by. The service takes both from VacuumTube and keeps them in a JSON file,
//! so they are known also while VacuumTube is closed.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::dlna::xml::element_text;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    /// With the prefix `uuid:`
    pub udn: String,
}

impl Identity {
    /// The identity in the description of a DIAL device
    pub fn from_description(xml: &str) -> Option<Identity> {
        Identity::checked(Identity {
            name: element_text(xml, "friendlyName")?,
            udn: element_text(xml, "UDN")?,
        })
    }

    /// The identity in a saved file. A file with wrong content gives None.
    pub fn load(file: &Path) -> Option<Identity> {
        let text = std::fs::read_to_string(file).ok()?;
        Identity::checked(serde_json::from_str(&text).ok()?)
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let json = serde_json::to_string_pretty(self).expect("the identity is valid JSON");
        std::fs::write(file, json + "\n")
    }

    fn checked(identity: Identity) -> Option<Identity> {
        (identity.udn.starts_with("uuid:") && !identity.name.is_empty()).then_some(identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        Identity {
            name: "Host (VacuumTube)".into(),
            udn: "uuid:1f9b969a-9cb0-41aa-9c40-4a452310f544".into(),
        }
    }

    fn temporary_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!(
                "htpc-control-identity-{name}-{}",
                std::process::id()
            ))
            .join("state/dial-identity.json")
    }

    #[test]
    fn saves_and_loads_the_identity() {
        let file = temporary_file("roundtrip");
        identity().save(&file).unwrap();
        assert_eq!(Identity::load(&file), Some(identity()));
        let _ = std::fs::remove_dir_all(file.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_missing_or_wrong_file_gives_no_identity() {
        let file = temporary_file("wrong");
        assert_eq!(Identity::load(&file), None);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        for content in [
            "not json",
            r#"{"name":"A","udn":"no uuid"}"#,
            r#"{"name":"","udn":"uuid:x"}"#,
        ] {
            std::fs::write(&file, content).unwrap();
            assert_eq!(Identity::load(&file), None, "{content}");
        }
        let _ = std::fs::remove_dir_all(file.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn reads_the_identity_from_a_description() {
        let xml = "<root><device><friendlyName>Host (VacuumTube)</friendlyName>\
                   <UDN>uuid:1f9b969a-9cb0-41aa-9c40-4a452310f544</UDN></device></root>";
        assert_eq!(Identity::from_description(xml), Some(identity()));
        assert_eq!(Identity::from_description("<root/>"), None);
    }
}
