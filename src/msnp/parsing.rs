//! Parsers for the list, presence and profile traffic the server pushes.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Online,
    Away,
    Busy,
    BeRightBack,
    Idle,
    OnThePhone,
    OutToLunch,
    Offline,
}

impl Status {
    pub fn from_code(code: &str) -> Option<Self> {
        match code.to_ascii_uppercase().as_str() {
            "NLN" => Some(Self::Online),
            "AWY" => Some(Self::Away),
            "BSY" => Some(Self::Busy),
            "BRB" => Some(Self::BeRightBack),
            "IDL" => Some(Self::Idle),
            "PHN" => Some(Self::OnThePhone),
            "LUN" => Some(Self::OutToLunch),
            "HDN" | "FLN" => Some(Self::Offline),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Online => "NLN",
            Self::Away => "AWY",
            Self::Busy => "BSY",
            Self::BeRightBack => "BRB",
            Self::Idle => "IDL",
            Self::OnThePhone => "PHN",
            Self::OutToLunch => "LUN",
            Self::Offline => "HDN",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::Away => "Away",
            Self::Busy => "Busy",
            Self::BeRightBack => "Be Right Back",
            Self::Idle => "Idle",
            Self::OnThePhone => "On The Phone",
            Self::OutToLunch => "Out To Lunch",
            Self::Offline => "Appear Offline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LstEntry {
    pub email: String,
    pub display_name: String,
    pub uuid: Option<String>,
    pub lists: u32,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    pub email: String,
    pub status: Status,
    pub display_name: String,
    pub client_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupEntry {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdcEntry {
    pub list: String,
    pub email: String,
    pub display_name: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

pub fn parse_lst<S: AsRef<str>>(args: &[S]) -> Result<LstEntry, ParseError> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    let args = strip_trid(&args);
    if args.is_empty() {
        return Err(ParseError("empty LST".into()));
    }

    if args[0].starts_with("N=") {
        let mut email = None;
        let mut display = String::new();
        let mut uuid = None;
        let mut list_at = None;

        for (index, arg) in args.iter().enumerate() {
            if let Some(value) = arg.strip_prefix("N=") {
                email = Some(percent_decode(value).to_ascii_lowercase());
            } else if let Some(value) = arg.strip_prefix("F=") {
                display = percent_decode(value);
            } else if let Some(value) = arg.strip_prefix("C=") {
                uuid = Some(value.to_string());
            } else if arg.parse::<u32>().is_ok() && list_at.is_none() {
                list_at = Some(index);
            }
        }

        let list_at = list_at.ok_or_else(|| ParseError("LST without list mask".into()))?;
        let lists = args[list_at]
            .parse::<u32>()
            .map_err(|_| ParseError("bad list mask".into()))?;
        let groups = args
            .get(list_at + 1)
            .map(|value| split_groups(value))
            .unwrap_or_default();

        Ok(LstEntry {
            email: email.ok_or_else(|| ParseError("LST without email".into()))?,
            display_name: display,
            uuid,
            lists,
            groups,
        })
    } else {
        if args.len() < 4 {
            return Err(ParseError("short LST".into()));
        }
        Ok(LstEntry {
            email: percent_decode(args[0]).to_ascii_lowercase(),
            display_name: percent_decode(args[1]),
            uuid: None,
            lists: args[2].parse().map_err(|_| ParseError("bad list mask".into()))?,
            groups: split_groups(args[3]),
        })
    }
}

pub fn parse_presence<S: AsRef<str>>(args: &[S]) -> Option<Presence> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    let email_at = args.iter().position(|arg| arg.contains('@'))?;
    if email_at == 0 {
        return None;
    }
    let status = Status::from_code(args[email_at - 1])?;
    let email = percent_decode(args[email_at]).to_ascii_lowercase();
    let display_name = args
        .get(email_at + 1)
        .map(|value| percent_decode(value))
        .unwrap_or_else(|| email.clone());
    let client_id = args.get(email_at + 2).and_then(|value| value.parse().ok());
    Some(Presence { email, status, display_name, client_id })
}

pub fn parse_lsg<S: AsRef<str>>(args: &[S]) -> Option<GroupEntry> {
    let mut args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    // Older fixtures and dialects put the group id first, sometimes behind a
    // transaction id. MSNP11 sends `LSG <name> <uuid>` with no transaction id.
    if args.len() >= 4 && args[0].parse::<u32>().is_ok() {
        args.remove(0);
    }
    if args.len() < 2 {
        return None;
    }

    let (id, name) = if is_uuid(args[1]) {
        (args[1].to_string(), args[0])
    } else if is_uuid(args[0]) {
        (args[0].to_string(), args[1])
    } else {
        (args[0].to_string(), args[1])
    };
    Some(GroupEntry {
        id,
        name: percent_decode(name),
    })
}

fn is_uuid(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    let lengths = [8, 4, 4, 4, 12];
    parts.len() == lengths.len()
        && parts
            .iter()
            .zip(lengths)
            .all(|(part, length)| part.len() == length && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

pub fn parse_ubx<S: AsRef<str>>(args: &[S], payload: &[u8]) -> Option<(String, Option<String>)> {
    let email = args.iter().find(|arg| arg.as_ref().contains('@'))?.as_ref();
    let text = String::from_utf8_lossy(payload);
    let psm = text
        .find("<PSM>")
        .and_then(|start| {
            let rest = &text[start + 5..];
            rest.find("</PSM>").map(|end| &rest[..end])
        })
        .map(unescape_xml);
    Some((percent_decode(email).to_ascii_lowercase(), psm))
}

pub fn parse_adc<S: AsRef<str>>(args: &[S]) -> Option<AdcEntry> {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    let args = strip_trid(&args);
    if args.len() < 2 {
        return None;
    }
    let list = args[0].to_string();
    let mut email = None;
    let mut display_name = None;
    let mut uuid = None;
    for arg in &args[1..] {
        if let Some(value) = arg.strip_prefix("N=") {
            email = Some(percent_decode(value).to_ascii_lowercase());
        } else if let Some(value) = arg.strip_prefix("F=") {
            display_name = Some(percent_decode(value));
        } else if let Some(value) = arg.strip_prefix("C=") {
            uuid = Some(value.to_string());
        }
    }
    Some(AdcEntry { list, email: email?, display_name, uuid })
}

fn strip_trid<'a>(args: &'a [&'a str]) -> &'a [&'a str] {
    if args.first().is_some_and(|arg| arg.parse::<u32>().is_ok()) {
        &args[1..]
    } else {
        args
    }
}

fn split_groups(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2])) {
                out.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
