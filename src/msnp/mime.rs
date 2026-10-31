//! MIME payloads carried by `MSG` on the notification and switchboard
//! servers. MSN Messenger 7.5 sends simple `text/plain` messages, control
//! messages for the typing indicator, and datacast payloads for nudges.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub text: String,
    pub content_type: String,
    pub headers: BTreeMap<String, String>,
    pub typing_user: Option<String>,
    pub datacast_id: Option<u32>,
}

pub fn build_im(text: &str) -> Vec<u8> {
    let body = text.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n");
    format!(
        "MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=UTF-8\r\n\
         X-MMS-IM-Format: FN=Segoe%20UI; EF=; CO=0; CS=0; PF=0\r\n\
         \r\n\
         {body}\r\n"
    )
    .into_bytes()
}

pub fn build_typing(email: &str) -> Vec<u8> {
    format!(
        "MIME-Version: 1.0\r\n\
         Content-Type: text/x-msmsgscontrol\r\n\
         TypingUser: {email}\r\n\
         \r\n\
         \r\n"
    )
    .into_bytes()
}

pub fn build_nudge() -> Vec<u8> {
    b"MIME-Version: 1.0\r\n\
      Content-Type: text/x-msnmsgr-datacast\r\n\
      \r\n\
      ID: 1\r\n\
      \r\n"
        .to_vec()
}

pub fn parse_payload(data: &[u8]) -> Incoming {
    let (raw_headers, raw_body) = split_headers(data);
    let mut headers = BTreeMap::new();
    for line in raw_headers.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            continue;
        };
        let key = String::from_utf8_lossy(&line[..colon]).trim().to_ascii_lowercase();
        let value = String::from_utf8_lossy(&line[colon + 1..]).trim().to_string();
        headers.insert(key, value);
    }

    let content_type = headers
        .get("content-type")
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();

    let transfer = headers
        .get("content-transfer-encoding")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    let body_bytes = if transfer.contains("quoted-printable") {
        decode_quoted_printable(raw_body)
    } else {
        raw_body.to_vec()
    };

    let charset = headers
        .get("content-type")
        .and_then(|value| charset_of(value))
        .unwrap_or_else(|| "utf-8".to_string());

    let mut text = decode_text(&body_bytes, &charset);
    while text.ends_with("\r\n") || text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }

    let typing_user = headers.get("typinguser").cloned().filter(|s| !s.is_empty());

    let datacast_id = if content_type == "text/x-msnmsgr-datacast" {
        text.lines()
            .find_map(|line| line.trim().strip_prefix("ID:").and_then(|id| id.trim().parse().ok()))
    } else {
        None
    };

    Incoming { text, content_type, headers, typing_user, datacast_id }
}

fn split_headers(data: &[u8]) -> (&[u8], &[u8]) {
    if let Some(pos) = find(data, b"\r\n\r\n") {
        (&data[..pos], &data[pos + 4..])
    } else if let Some(pos) = find(data, b"\n\n") {
        (&data[..pos], &data[pos + 2..])
    } else {
        (data, &[])
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn charset_of(value: &str) -> Option<String> {
    value.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("charset=").map(|charset| charset.trim_matches('"').to_ascii_lowercase())
    })
}

fn decode_text(bytes: &[u8], charset: &str) -> String {
    match charset {
        "iso-8859-1" | "latin1" | "windows-1252" => bytes.iter().map(|byte| *byte as char).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn decode_quoted_printable(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut index = 0;

    while index < data.len() {
        if data[index] != b'=' {
            out.push(data[index]);
            index += 1;
            continue;
        }

        if data.get(index + 1) == Some(&b'\r') && data.get(index + 2) == Some(&b'\n') {
            index += 3;
            continue;
        }
        if data.get(index + 1) == Some(&b'\n') {
            index += 2;
            continue;
        }
        if let (Some(high), Some(low)) = (data.get(index + 1), data.get(index + 2)) {
            if let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low)) {
                out.push(high << 4 | low);
                index += 3;
                continue;
            }
        }

        out.push(b'=');
        index += 1;
    }

    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
