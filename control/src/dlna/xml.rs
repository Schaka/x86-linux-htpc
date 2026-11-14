// SPDX-License-Identifier: AGPL-3.0-only
//! The small part of XML that SOAP requests and DIDL-Lite metadata need.

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text of the first element with this local name, with or without a
/// namespace prefix. The text is unescaped, and a CDATA section is taken
/// as it is.
pub fn element_text(xml: &str, local_name: &str) -> Option<String> {
    let mut position = 0;
    while let Some(offset) = xml[position..].find('<') {
        let start = position + offset + 1;
        let name_end = xml[start..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|e| start + e)?;
        let full_name = &xml[start..name_end];
        let local = full_name.rsplit(':').next().unwrap_or(full_name);
        let tag_end = start + xml[start..].find('>')?;
        position = tag_end + 1;
        if local != local_name || full_name.starts_with('/') {
            continue;
        }
        if xml[..tag_end].ends_with('/') {
            return Some(String::new());
        }
        let close = format!("</{full_name}>");
        let end = position + xml[position..].find(&close)?;
        let content = &xml[position..end];
        let trimmed = content.trim();
        if let Some(cdata) = trimmed
            .strip_prefix("<![CDATA[")
            .and_then(|c| c.strip_suffix("]]>"))
        {
            return Some(cdata.to_string());
        }
        return Some(unescape(content));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_unescapes() {
        let text = r#"<a href="x">Tom & Jerry's</a>"#;
        assert_eq!(unescape(&escape(text)), text);
        assert_eq!(unescape("&#65;&#x42;&unknown; &"), "AB&unknown; &");
    }

    #[test]
    fn reads_soap_arguments() {
        let body = r#"<s:Envelope><s:Body><u:SetAVTransportURI xmlns:u="urn:x">
            <InstanceID>0</InstanceID><CurrentURI>http://h/a?b=1&amp;c=2</CurrentURI>
            <CurrentURIMetaData>&lt;DIDL-Lite&gt;&lt;dc:title&gt;Film&lt;/dc:title&gt;&lt;/DIDL-Lite&gt;</CurrentURIMetaData>
            <Empty/></u:SetAVTransportURI></s:Body></s:Envelope>"#;
        assert_eq!(
            element_text(body, "CurrentURI").as_deref(),
            Some("http://h/a?b=1&c=2")
        );
        let metadata = element_text(body, "CurrentURIMetaData").unwrap();
        assert_eq!(element_text(&metadata, "title").as_deref(), Some("Film"));
        assert_eq!(element_text(body, "Empty").as_deref(), Some(""));
        assert_eq!(element_text(body, "Missing"), None);
        assert_eq!(
            element_text("<a><![CDATA[x<y]]></a>", "a").as_deref(),
            Some("x<y")
        );
    }
}
