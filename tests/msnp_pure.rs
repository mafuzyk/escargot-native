//! Behavioural tests for the wire-level pieces of the MSNP11 client.
//! Everything here runs without a network or a GUI.

use std::sync::{Arc, Mutex};

use escargot_native::msnp::auth::{
    HttpTransport, HttpResponse, PassportAuth, AuthError, build_authorization_header,
    extract_ticket, parse_passport_urls,
};
use escargot_native::msnp::framing::{Command, Framer};
use escargot_native::msnp::mime::{build_im, build_nudge, build_typing, parse_payload};
use escargot_native::msnp::normalize_account;
use escargot_native::msnp::parsing::{
    Status, parse_adc, parse_lst, parse_lsg, parse_presence, parse_ubx,
};

fn feed(framer: &mut Framer, bytes: &[u8]) -> Vec<Command> {
    framer.feed(bytes)
}

#[test]
fn frames_a_complete_line() {
    let mut framer = Framer::default();
    let out = feed(&mut framer, b"VER 0 MSNP11 CVR0\r\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].verb, "VER");
    assert_eq!(out[0].args, vec!["0", "MSNP11", "CVR0"]);
    assert!(out[0].payload.is_none());
}

#[test]
fn waits_for_partial_line() {
    let mut framer = Framer::default();
    assert!(feed(&mut framer, b"VER 0 MS").is_empty());
    let out = feed(&mut framer, b"NP11 CVR0\r\n");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].verb, "VER");
}

#[test]
fn reads_payload_with_embedded_crlf() {
    let mut framer = Framer::default();
    let body = b"MIME-Version: 1.0\r\n\r\nhello\r\nworld\r\n";
    let mut wire = format!("MSG 5 N {}\r\n", body.len()).into_bytes();
    wire.extend_from_slice(body);
    let out = feed(&mut framer, &wire);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].payload.as_deref(), Some(body.as_slice()));
}

#[test]
fn reads_command_after_payload() {
    let mut framer = Framer::default();
    let body = b"MIME-Version: 1.0\r\n\r\nhi\r\n";
    let mut wire = format!("MSG a@b.com Friend {}\r\n", body.len()).into_bytes();
    wire.extend_from_slice(body);
    wire.extend_from_slice(b"FLN a@b.com\r\n");
    let out = feed(&mut framer, &wire);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].verb, "MSG");
    assert_eq!(out[1].verb, "FLN");
}

#[test]
fn payload_length_counts_bytes_not_chars() {
    let body = "olá".as_bytes();
    let mut wire = format!("MSG 1 U {}\r\n", body.len()).into_bytes();
    wire.extend_from_slice(body);
    let mut framer = Framer::default();
    let out = feed(&mut framer, &wire);
    assert_eq!(out[0].payload.as_deref(), Some(body));
}

#[test]
fn parses_optional_transaction_id() {
    let cmd = Command::parse("LST 12 N=a@b.com F=Bob 1 0".to_string(), None);
    assert_eq!(cmd.trid(), Some(12));
    assert_eq!(
        cmd.params(),
        ["N=a@b.com".to_string(), "F=Bob".to_string(), "1".to_string(), "0".to_string()].as_slice()
    );
}

#[test]
fn builds_a_plain_text_message() {
    let payload = build_im("hi there");
    let text = String::from_utf8(payload.clone()).unwrap();
    assert!(text.starts_with("MIME-Version: 1.0\r\n"));
    assert!(text.contains("Content-Type: text/plain; charset=UTF-8"));
    assert!(text.contains("X-MMS-IM-Format:"));
    assert!(text.ends_with("\r\n\r\nhi there\r\n"));
}

#[test]
fn normalizes_newlines_in_outgoing_messages() {
    let payload = build_im("one\ntwo");
    assert!(payload.windows(4).any(|w| w == b"one\r"));
    assert!(!String::from_utf8(payload).unwrap().contains("one\ntwo"));
}

#[test]
fn parses_incoming_message() {
    let payload = b"MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nX-MMS-IM-Format: FN=Segoe%20UI; EF=; CO=0; CS=0; PF=0\r\n\r\nhello world\r\n";
    let msg = parse_payload(payload);
    assert_eq!(msg.text, "hello world");
    assert_eq!(msg.content_type, "text/plain");
    assert!(msg.typing_user.is_none());
    assert!(msg.datacast_id.is_none());
}

#[test]
fn decodes_quoted_printable_bodies() {
    let payload = b"MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nol=C3=A1 mundo\r\n";
    let msg = parse_payload(payload);
    assert_eq!(msg.text, "olá mundo");
}

#[test]
fn parses_typing_notification() {
    let payload = build_typing("friend@example.com");
    let msg = parse_payload(&payload);
    assert_eq!(msg.typing_user.as_deref(), Some("friend@example.com"));
    assert_eq!(msg.text, "");
}

#[test]
fn parses_nudge() {
    let payload = build_nudge();
    let msg = parse_payload(&payload);
    assert_eq!(msg.datacast_id, Some(1));
}

#[test]
fn parses_modern_list_entry() {
    let entry = parse_lst(&["5", "N=Friend@Example.com", "F=Friend%20Name", "C=uuid-1", "1", "0"]).unwrap();
    assert_eq!(entry.email, "friend@example.com".to_lowercase());
    assert_eq!(entry.display_name, "Friend Name");
    assert_eq!(entry.uuid.as_deref(), Some("uuid-1"));
    assert_eq!(entry.lists, 1);
    assert_eq!(entry.groups, vec!["0"]);
}

#[test]
fn parses_legacy_list_entry() {
    let entry = parse_lst(&["5", "friend@example.com", "Friend", "2", "group-1"]).unwrap();
    assert_eq!(entry.email, "friend@example.com");
    assert_eq!(entry.display_name, "Friend");
    assert!(entry.uuid.is_none());
    assert_eq!(entry.lists, 2);
    assert_eq!(entry.groups, vec!["group-1"]);
}

#[test]
fn parses_group_entry() {
    let group = parse_lsg(&["9", "00000000-0000-0000-0001-000000000001", "Friends", "0"]).unwrap();
    assert_eq!(group.id, "00000000-0000-0000-0001-000000000001");
    assert_eq!(group.name, "Friends");
}

#[test]
fn parses_the_msnp11_group_entry_order() {
    let group = parse_lsg(&["Friends", "00000000-0000-0000-0001-000000000001"]).unwrap();
    assert_eq!(
        group.id, "00000000-0000-0000-0001-000000000001",
        "MSNP11 sends LSG <name> <uuid>; the UUID is the list identifier"
    );
    assert_eq!(group.name, "Friends", "The group name must stay readable");
}

#[test]
fn parses_online_and_away_presence() {
    let online = parse_presence(&["6", "NLN", "friend@example.com", "Friend%20Name", "268435456"]).unwrap();
    assert_eq!(online.email, "friend@example.com");
    assert_eq!(online.status, Status::Online);
    assert_eq!(online.display_name, "Friend Name");

    let away = parse_presence(&["AWY", "friend@example.com", "Friend%20Name"]).unwrap();
    assert_eq!(away.status, Status::Away);
    assert_eq!(away.client_id, None);
}

#[test]
fn maps_status_codes() {
    assert_eq!(Status::from_code("NLN"), Some(Status::Online));
    assert_eq!(Status::from_code("BSY"), Some(Status::Busy));
    assert_eq!(Status::from_code("BRB"), Some(Status::BeRightBack));
    assert_eq!(Status::from_code("IDL"), Some(Status::Idle));
    assert_eq!(Status::from_code("PHN"), Some(Status::OnThePhone));
    assert_eq!(Status::from_code("LUN"), Some(Status::OutToLunch));
    assert_eq!(Status::from_code("AWY"), Some(Status::Away));
    assert_eq!(Status::from_code("HDN"), Some(Status::Offline));
    assert_eq!(Status::from_code("???"), None);
}

#[test]
fn reads_personal_message_from_ubx() {
    let payload = b"<Data><PSM>listening to Aphex Twin &amp; friends</PSM><CurrentMedia></CurrentMedia></Data>";
    let (email, psm) = parse_ubx(&["7", "friend@example.com", "76"], payload).unwrap();
    assert_eq!(email, "friend@example.com");
    assert_eq!(psm.as_deref(), Some("listening to Aphex Twin & friends"));
}

#[test]
fn parses_added_contact() {
    let adc = parse_adc(&["8", "FL", "N=a@b.com", "F=Alice", "C=uuid-2"]).unwrap();
    assert_eq!(adc.list, "FL");
    assert_eq!(adc.email, "a@b.com");
    assert_eq!(adc.display_name.as_deref(), Some("Alice"));
    assert_eq!(adc.uuid.as_deref(), Some("uuid-2"));
}

#[test]
fn parses_passport_urls() {
    let urls = parse_passport_urls(
        "DALogin=https://ds.example/login,DAReg=http://example/reg,Properties=https://account.example/",
    );
    assert_eq!(urls.get("DALogin").map(String::as_str), Some("https://ds.example/login"));
    assert_eq!(urls.get("Properties").map(String::as_str), Some("https://account.example/"));
}

#[test]
fn encodes_credentials_in_passport_header() {
    let header = build_authorization_header("alice@example.com", "p,ss word&x");
    assert!(header.starts_with("Passport1.4 "));
    assert!(header.contains("sign-in=alice%40example.com"));
    assert!(header.contains("pwd=p%2Css%20word%26x"));
}

#[test]
fn extracts_ticket_from_authentication_info() {
    let token = extract_ticket("Passport1.4 da-status=success,from-PP='t=abcdefghijklmnopqrst'").unwrap();
    assert_eq!(token, "abcdefghijklmnopqrst");
}

#[test]
fn extracts_untagged_ticket() {
    let token = extract_ticket("Passport1.4 da-status=success,from-PP='abcdefghijklmnopqrst'").unwrap();
    assert_eq!(token, "abcdefghijklmnopqrst");
}

struct FakeTransport {
    responses: Mutex<Vec<HttpResponse>>,
    requests: Mutex<Vec<(String, Vec<(String, String)>)>>,
}

impl FakeTransport {
    fn new(responses: Vec<HttpResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        })
    }
}

impl HttpTransport for FakeTransport {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, AuthError> {
        self.requests.lock().unwrap().push((
            url.to_string(),
            headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ));
        self.responses
            .lock()
            .unwrap()
            .remove(0)
            .into_ok()
    }
}

trait IntoOk {
    fn into_ok(self) -> Result<HttpResponse, AuthError>;
}

impl IntoOk for HttpResponse {
    fn into_ok(self) -> Result<HttpResponse, AuthError> {
        Ok(self)
    }
}

fn response(status: u16, headers: &[(&str, &str)], body: &str) -> HttpResponse {
    HttpResponse {
        status,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn follows_nexus_and_returns_ticket() {
    let transport = FakeTransport::new(vec![
        response(
            200,
            &[("PassportURLs", "DALogin=https://ds.example/login,DAReg=x")],
            "",
        ),
        response(
            200,
            &[(
                "Authentication-Info",
                "Passport1.4 da-status=success,from-PP='t=abcdefghijklmnopqrst'",
            )],
            "",
        ),
    ]);
    let auth = PassportAuth::with_transport("https://msn.example", Box::new(transport.clone()));
    let token = auth.fetch_ticket("alice@example.com", "secret").unwrap();
    assert_eq!(token, "abcdefghijklmnopqrst");

    let reqs = transport.requests.lock().unwrap();
    assert_eq!(reqs[0].0, "https://msn.example/rdr/pprdr.asp");
    assert_eq!(reqs[1].0, "https://ds.example/login");
    assert!(
        reqs[1]
            .1
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("Authorization")
                && v.contains("sign-in=alice%40example.com"))
    );
}

#[test]
fn reports_invalid_credentials() {
    let transport = FakeTransport::new(vec![
        response(
            200,
            &[("PassportURLs", "DALogin=https://ds.example/login")],
            "",
        ),
        response(401, &[("WWW-Authenticate", "Passport1.4 da-status=failed")], ""),
    ]);
    let auth = PassportAuth::with_transport("https://msn.example", Box::new(transport));
    let err = auth.fetch_ticket("alice@example.com", "wrong").unwrap_err();
    assert!(matches!(err, AuthError::InvalidCredentials));
}

#[test]
fn normalizes_escargot_usernames() {
    assert_eq!(normalize_account("mafuzik"), "mafuzik@escargot.chat");
    assert_eq!(normalize_account("  Mafuzik  "), "mafuzik@escargot.chat");
    assert_eq!(normalize_account("mafuzik@"), "mafuzik@escargot.chat");
    assert_eq!(normalize_account("mafuzik@escargot.chat"), "mafuzik@escargot.chat");
    assert_eq!(normalize_account("a@b.example"), "a@b.example");
    assert_eq!(normalize_account(""), "");
}

#[test]
fn contact_accounts_reject_malformed_or_injected_commands() {
    for input in ["", "@escargot.chat", "friend@@escargot.chat", "friend other", "friend\r\nCHG 1 NLN", "friend\0@escargot.chat"] {
        assert!(
            escargot_native::msnp::normalize_contact_account(input).is_err(),
            "Invalid contact account must be rejected before a wire command: {input:?}"
        );
    }
}

#[test]
fn a_contact_notification_payload_cannot_swallow_the_next_switchboard_reply() {
    let mut framer = Framer::default();
    let payload = b"<NOTIFICATION><MSG>Contact list changed</MSG></NOTIFICATION>";
    let header = format!("NOT {}\r\n", payload.len());
    assert!(framer.feed(header.as_bytes()).is_empty(), "NOT must wait for its payload");
    let mut wire = payload.to_vec();
    wire.extend_from_slice(b"XFR 10 SB localhost:1864 CKI test-token\r\n");
    let commands = framer.feed(&wire);
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].payload.as_deref(), Some(payload.as_slice()));
    assert_eq!(commands[1].verb, "XFR", "Contact notifications must not corrupt subsequent protocol commands");
}
