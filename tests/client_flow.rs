//! End-to-end protocol test: a fake Escargot notification server, switchboard
//! and Passport endpoint, with the real client on the other side. This proves
//! the sign-in, contact list, switchboard and both message directions work
//! before any credentials touch the live service.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use escargot_native::msnp::client::{ClientConfig, ClientEvent, EventSink, start};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, sleep};

async fn read_line(stream: &mut TcpStream) -> String {
    let mut line = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await.unwrap();
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] != b'\r' {
            line.push(byte[0]);
        }
    }
    String::from_utf8_lossy(&line).into_owned()
}

async fn read_payload(stream: &mut TcpStream, length: usize) -> Vec<u8> {
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).await.unwrap();
    payload
}

async fn send(stream: &mut TcpStream, text: &str) {
    stream.write_all(text.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

async fn spawn_auth() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request).await;
            let response = if port != 0 {
                format!(
                    "HTTP/1.1 200 OK\r\nPassportURLs: DALogin=http://127.0.0.1:{port}/login\r\nAuthentication-Info: Passport1.4 da-status=success,from-PP='t=abcdefghijklmnopqrst'\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
            } else {
                String::new()
            };
            stream.write_all(response.as_bytes()).await.ok();
            stream.flush().await.ok();
        }
    });
    port
}

async fn spawn_switchboard(received: Arc<AtomicBool>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_line(&mut stream).await, "USR 1 me@example.com token123", "A switchboard starts with USR, without notification-server version negotiation");
        send(&mut stream, "USR 1 OK me@example.com Online\r\n").await;
        let cal = read_line(&mut stream).await;
        assert!(cal.starts_with("CAL 2 friend@example.com"));
        send(&mut stream, "CAL 2 RINGING 42\r\n").await;
        send(&mut stream, "JOI friend@example.com Friend\r\n").await;

        let header = read_line(&mut stream).await;
        assert!(header.starts_with("MSG 3 N "));
        let length: usize = header.split(' ').last().unwrap().parse().unwrap();
        let payload = read_payload(&mut stream, length).await;
        let text = String::from_utf8_lossy(&payload);
        assert!(text.contains("hello from linux"), "payload was {text:?}");
        received.store(true, Ordering::SeqCst);

        let incoming = b"MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\nhello from windows\r\n";
        send(
            &mut stream,
            &format!("MSG friend@example.com Friend {}\r\n", incoming.len()),
        )
        .await;
        stream.write_all(incoming).await.unwrap();
        stream.flush().await.unwrap();

        sleep(Duration::from_secs(5)).await;
    });

    port
}

async fn spawn_notification(sb_port: u16) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        assert_eq!(read_line(&mut stream).await, "VER 0 MSNP11 CVR0");
        send(&mut stream, "VER 0 MSNP11\r\n").await;

        assert!(read_line(&mut stream).await.starts_with("CVR 1 "));
        send(&mut stream, "CVR 1 7.5.0324 7.5.0324 7.5.0324 https://escargot.chat https://escargot.chat\r\n").await;

        assert_eq!(read_line(&mut stream).await, "USR 2 TWN I me@example.com");
        send(&mut stream, "USR 2 TWN S ct=1,rver=1,wp=FS_40SEC_0_COMPACT,lc=1,id=1\r\n").await;

        assert_eq!(read_line(&mut stream).await, "USR 3 TWN S t=abcdefghijklmnopqrst");
        send(&mut stream, "USR 3 OK me@example.com 1 0\r\n").await;

        assert_eq!(read_line(&mut stream).await, "SYN 4 0");
        send(&mut stream, "SYN 4 1750000000 1750000000 1 2\r\n").await;
        send(&mut stream, "GTC A\r\n").await;
        send(&mut stream, "BLP AL\r\n").await;
        send(&mut stream, "PRP MFN Me%20Name\r\n").await;
        send(&mut stream, "LSG 0 Other Contacts 0\r\n").await;
        send(&mut stream, "LSG group-1 Friends 0\r\n").await;
        send(&mut stream, "LST N=friend@example.com F=Friend C=uuid-friend 1 group-1\r\n").await;
        send(&mut stream, "LST N=other@example.com F=Other C=uuid-other 2 0\r\n").await;

        assert_eq!(read_line(&mut stream).await, "CHG 5 NLN");
        send(&mut stream, "CHG 5 NLN\r\n").await;

        let xfr = read_line(&mut stream).await;
        assert!(xfr.starts_with("XFR 10 SB"), "unexpected command {xfr}");
        send(
            &mut stream,
            &format!("XFR 10 SB 127.0.0.1:{sb_port} CKI token123\r\n"),
        )
        .await;

        sleep(Duration::from_secs(8)).await;
    });

    port
}

#[tokio::test]
async fn signs_in_opens_a_switchboard_and_sends_messages_both_ways() {
    let received = Arc::new(AtomicBool::new(false));
    let auth_port = spawn_auth().await;
    let sb_port = spawn_switchboard(received.clone()).await;
    let ns_port = spawn_notification(sb_port).await;

    let sink = EventSink::new();
    let handle = start(
        ClientConfig {
            email: "me@example.com".into(),
            password: "secret".into(),
            server: "127.0.0.1".into(),
            port: ns_port,
            login_host: format!("http://127.0.0.1:{auth_port}"),
        },
        sink.clone(),
    );

    let deadline = Instant::now() + Duration::from_secs(12);
    let mut signed_in = false;
    let mut contact_list = false;
    let mut incoming_message = false;
    let mut opened = false;

    while Instant::now() < deadline && !(incoming_message && received.load(Ordering::SeqCst)) {
        for event in sink.drain() {
            match event {
                ClientEvent::SignedIn { .. } => signed_in = true,
                ClientEvent::ContactList { contacts, .. } => {
                    contact_list = contacts.iter().any(|contact| contact.email == "friend@example.com");
                }
                ClientEvent::ChatReady { .. } => {
                    if !opened {
                        opened = true;
                        handle.send_message("friend@example.com", "hello from linux".into());
                    }
                }
                ClientEvent::Message { text, .. } => {
                    if text == "hello from windows" {
                        incoming_message = true;
                    }
                }
                _ => {}
            }
        }
        if signed_in && contact_list && !opened {
            opened = true;
            handle.open_chat("friend@example.com");
            handle.send_message("friend@example.com", "hello from linux".into());
        }
        sleep(Duration::from_millis(40)).await;
    }

    assert!(signed_in, "client never reached the signed-in state");
    assert!(contact_list, "client never received the contact list");
    assert!(incoming_message, "client never surfaced the incoming message");
    assert!(
        received.load(Ordering::SeqCst),
        "the switchboard never received the outgoing message"
    );
}

async fn contact_session() -> (
    escargot_native::msnp::client::ClientHandle,
    EventSink,
    TcpStream,
) {
    contact_session_with("me@example.com", "").await
}

async fn contact_session_with(email: &str, list: &str) -> (
    escargot_native::msnp::client::ClientHandle,
    EventSink,
    TcpStream,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth_port = spawn_auth().await;
    let sink = EventSink::new();
    let client = start(
        ClientConfig {
            email: email.into(),
            password: "test-password".into(),
            server: "127.0.0.1".into(),
            port,
            login_host: format!("http://127.0.0.1:{auth_port}"),
        },
        sink.clone(),
    );
    let (mut stream, _) = listener.accept().await.unwrap();
    assert_eq!(read_line(&mut stream).await, "VER 0 MSNP11 CVR0");
    send(&mut stream, "VER 0 MSNP11\r\n").await;
    assert!(read_line(&mut stream).await.starts_with("CVR 1 "));
    send(&mut stream, "CVR 1 7.5.0324\r\n").await;
    assert_eq!(read_line(&mut stream).await, format!("USR 2 TWN I {email}"));
    send(&mut stream, "USR 2 TWN S challenge\r\n").await;
    assert_eq!(read_line(&mut stream).await, "USR 3 TWN S t=abcdefghijklmnopqrst");
    send(&mut stream, &format!("USR 3 OK {email} 1 0\r\n")).await;
    assert_eq!(read_line(&mut stream).await, "SYN 4 0");
    send(&mut stream, "SYN 4 1 1 0 0\r\n").await;
    send(&mut stream, list).await;
    assert_eq!(read_line(&mut stream).await, "CHG 5 NLN");
    send(&mut stream, "CHG 5 NLN\r\n").await;
    loop {
        let events = sink.drain();
        let signed_in = events.iter().any(|event| matches!(event, ClientEvent::SignedIn { .. }));
        for event in events {
            if !matches!(event, ClientEvent::SignedIn { .. }) { sink.push(event); }
        }
        if signed_in { break; }
        sleep(Duration::from_millis(5)).await;
    }
    (client, sink, stream)
}

#[tokio::test]
async fn adds_a_contact_natively_and_waits_for_both_server_acknowledgements() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut stream) = contact_session().await;
        client.add_contact(" NewFriend ").unwrap();
        assert_eq!(
            read_line(&mut stream).await,
            "ADC 10 FL N=newfriend@escargot.chat F=newfriend%40escargot.chat"
        );
        send(&mut stream, "ADC 10 FL N=newfriend@escargot.chat F=New%20Friend C=contact-uuid\r\n").await;
        assert_eq!(read_line(&mut stream).await, "ADC 11 AL N=newfriend@escargot.chat");
        let before_allow = sink.drain();
        assert!(
            !before_allow.iter().any(|event| matches!(event, ClientEvent::ContactAdded { .. })),
            "The client must wait for the allow-list acknowledgement before reporting success"
        );
        send(&mut stream, "ADC 11 AL N=newfriend@escargot.chat\r\n").await;
        let mut added = false;
        let mut listed = before_allow.iter().any(|event| matches!(event,
            ClientEvent::ContactUpdated(info) if info.email == "newfriend@escargot.chat" && info.display_name == "New Friend"
        ));
        while !added || !listed {
            for event in sink.drain() {
                match event {
                    ClientEvent::ContactAdded { email } => {
                        assert_eq!(email, "newfriend@escargot.chat");
                        added = true;
                    }
                    ClientEvent::ContactUpdated(info) if info.email == "newfriend@escargot.chat" => {
                        assert_eq!(info.display_name, "New Friend");
                        listed = true;
                    }
                    ClientEvent::Error(message) | ClientEvent::Disconnected(message) => panic!("The existing session must stay connected: {message}"),
                    _ => {}
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut stream).await, "CHG 12 BSY");
    }).await.expect("Native contact addition must finish without reconnecting");
}

async fn contact_result(sink: &EventSink) -> ClientEvent {
    loop {
        for event in sink.drain() {
            match event {
                ClientEvent::ContactAdded { .. } | ClientEvent::ContactAddFailed { .. } => return event,
                ClientEvent::Error(message) | ClientEvent::Disconnected(message) => panic!("Contact operations must keep the session connected: {message}"),
                _ => {}
            }
        }
        sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn an_unknown_contact_reports_the_server_rejection_without_reconnecting() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut stream) = contact_session().await;
        client.add_contact("missing").unwrap();
        assert!(read_line(&mut stream).await.starts_with("ADC 10 FL N=missing@escargot.chat "));
        send(&mut stream, "208 10\r\n").await;
        match contact_result(&sink).await {
            ClientEvent::ContactAddFailed { email, message } => {
                assert_eq!(email, "missing@escargot.chat");
                assert!(message.contains("does not exist"));
            }
            event => panic!("A rejected contact must not be reported as added: {event:?}"),
        }
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut stream).await, "CHG 11 BSY");
    }).await.expect("A contact rejection must not disconnect or stall the session");
}

#[tokio::test]
async fn a_partial_contact_add_can_be_retried_after_status_sharing_fails() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut stream) = contact_session().await;
        client.add_contact("newfriend").unwrap();
        assert!(read_line(&mut stream).await.starts_with("ADC 10 FL "));
        send(&mut stream, "ADC 10 FL N=newfriend@escargot.chat F=New%20Friend C=contact-uuid\r\n").await;
        assert_eq!(read_line(&mut stream).await, "ADC 11 AL N=newfriend@escargot.chat");
        send(&mut stream, "500 11\r\n").await;
        match contact_result(&sink).await {
            ClientEvent::ContactAddFailed { message, .. } => assert!(message.contains("contact was saved")),
            event => panic!("Partial completion must be explained: {event:?}"),
        }
        client.add_contact("newfriend").unwrap();
        assert!(read_line(&mut stream).await.starts_with("ADC 12 FL "));
        send(&mut stream, "ADC 12 FL N=newfriend@escargot.chat F=New%20Friend C=contact-uuid\r\n").await;
        assert_eq!(read_line(&mut stream).await, "ADC 13 AL N=newfriend@escargot.chat");
        send(&mut stream, "ADC 13 AL N=newfriend@escargot.chat\r\n").await;
        assert!(matches!(contact_result(&sink).await, ClientEvent::ContactAdded { email } if email == "newfriend@escargot.chat"));
    }).await.expect("A partial contact addition must remain recoverable");
}

#[tokio::test]
async fn contact_confirmation_for_another_account_does_not_report_success() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut stream) = contact_session().await;
        client.add_contact("newfriend").unwrap();
        assert!(read_line(&mut stream).await.starts_with("ADC 10 FL "));
        send(&mut stream, "ADC 10 FL N=other@escargot.chat F=Other C=wrong-uuid\r\n").await;
        assert!(matches!(contact_result(&sink).await, ClientEvent::ContactAddFailed { email, .. } if email == "newfriend@escargot.chat"));
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut stream).await, "CHG 11 BSY");
    }).await.expect("The server must confirm the requested account");
}

#[tokio::test]
async fn unanswered_contact_requests_expire_without_closing_the_session() {
    tokio::time::timeout(Duration::from_secs(23), async {
        let (client, sink, mut stream) = contact_session().await;
        client.add_contact("newfriend").unwrap();
        assert!(read_line(&mut stream).await.starts_with("ADC 10 FL "));
        match contact_result(&sink).await {
            ClientEvent::ContactAddFailed { email, message } => {
                assert_eq!(email, "newfriend@escargot.chat");
                assert!(message.contains("did not confirm"));
            }
            event => panic!("An unanswered request must not succeed: {event:?}"),
        }
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut stream).await, "CHG 11 BSY");
        send(&mut stream, "ADC 10 FL N=newfriend@escargot.chat F=New%20Friend C=contact-uuid\r\n").await;
        client.set_status(escargot_native::msnp::parsing::Status::Online);
        assert_eq!(read_line(&mut stream).await, "CHG 12 NLN", "A late acknowledgement must not restart an expired request");
    }).await.expect("A contact request must stop waiting after its confirmation deadline");
}

#[tokio::test]
async fn contact_requests_during_sign_in_are_rejected_instead_of_silently_dropped() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let sink = EventSink::new();
        let client = start(ClientConfig {
            email: "me@example.com".into(), password: "test-password".into(),
            server: "127.0.0.1".into(), port: listener.local_addr().unwrap().port(),
            login_host: "http://127.0.0.1".into(),
        }, sink.clone());
        let (mut stream, _) = listener.accept().await.unwrap();
        assert_eq!(read_line(&mut stream).await, "VER 0 MSNP11 CVR0");
        client.add_contact("newfriend").unwrap();
        match contact_result(&sink).await {
            ClientEvent::ContactAddFailed { message, .. } => assert!(message.contains("sign-in to finish")),
            event => panic!("A request made before authentication needs an explicit result: {event:?}"),
        }
    }).await.expect("A request during sign-in must not be silently discarded");
}

#[tokio::test]
async fn messages_continue_while_contact_confirmation_is_pending() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let received = Arc::new(AtomicBool::new(false));
        let sb_port = spawn_switchboard(received.clone()).await;
        let (client, sink, mut stream) = contact_session().await;
        client.add_contact("newfriend").unwrap();
        assert!(read_line(&mut stream).await.starts_with("ADC 10 FL "));
        client.send_message("friend@example.com", "hello from linux".into());
        assert_eq!(read_line(&mut stream).await, "XFR 11 SB");
        send(&mut stream, &format!("XFR 11 SB 127.0.0.1:{sb_port} CKI token123\r\n")).await;
        let mut incoming = false;
        while !incoming || !received.load(Ordering::SeqCst) {
            for event in sink.drain() {
                match event {
                    ClientEvent::Message { text, .. } if text == "hello from windows" => incoming = true,
                    ClientEvent::ContactAdded { .. } => panic!("The contact has not been confirmed yet"),
                    ClientEvent::Error(message) | ClientEvent::Disconnected(message) => panic!("Messaging must stay connected: {message}"),
                    _ => {}
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
        send(&mut stream, "ADC 10 FL N=newfriend@escargot.chat F=New%20Friend C=contact-uuid\r\n").await;
        assert_eq!(read_line(&mut stream).await, "ADC 12 AL N=newfriend@escargot.chat");
        send(&mut stream, "ADC 12 AL N=newfriend@escargot.chat\r\n").await;
        assert!(matches!(contact_result(&sink).await, ClientEvent::ContactAdded { .. }));
    }).await.expect("Adding a contact must not block sending or receiving messages");
}

async fn spawn_unavailable_switchboard() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let usr = read_line(&mut stream).await;
        assert!(usr.starts_with("USR 1 me@example.com "), "unexpected USR {usr}");
        send(&mut stream, "USR 1 OK me@example.com Me\r\n").await;
        let cal = read_line(&mut stream).await;
        assert!(cal.starts_with("CAL 2 friend@example.com"), "unexpected CAL {cal}");
        send(&mut stream, "217 2\r\n").await;
        sleep(Duration::from_secs(3)).await;
    });
    port
}

#[tokio::test]
async fn an_offline_invitee_is_reported_without_waiting_for_the_read_timeout() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let sb_port = spawn_unavailable_switchboard().await;
        let (client, sink, mut ns) = contact_session().await;
        client.send_message("friend@example.com", "are you there?".into());
        assert_eq!(read_line(&mut ns).await, "XFR 10 SB");
        send(
            &mut ns,
            &format!("XFR 10 SB 127.0.0.1:{sb_port} CKI token123\r\n"),
        )
        .await;

        let started = Instant::now();
        loop {
            for event in sink.drain() {
                if let ClientEvent::ChatError { message, .. } = event {
                    assert!(
                        message.to_ascii_lowercase().contains("offline"),
                        "An offline invitee needs an actionable message, got {message:?}"
                    );
                    assert!(
                        !message.to_ascii_lowercase().contains("timed out"),
                        "A numeric CAL rejection must not be reported as a timeout"
                    );
                    assert!(
                        started.elapsed() < Duration::from_secs(3),
                        "The failure must be reported immediately, not after the read timeout"
                    );
                    client.set_status(escargot_native::msnp::parsing::Status::Busy);
                    assert_eq!(read_line(&mut ns).await, "CHG 11 BSY");
                    return;
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("An offline invitee must fail fast without a generic timeout");
}

async fn expect_message(sink: &EventSink, expected_peer: &str, expected_text: &str) {
    loop {
        for event in sink.drain() {
            match event {
                ClientEvent::Message { peer, text, .. } if peer == expected_peer && text == expected_text => return,
                ClientEvent::ChatError { message, .. } | ClientEvent::Disconnected(message) => panic!("Message delivery failed: {message}"),
                _ => {},
            }
        }
        sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn outgoing_messages_are_confirmed_after_the_switchboard_write() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let received = Arc::new(AtomicBool::new(false));
        let auth_port = spawn_auth().await;
        let sb_port = spawn_switchboard(received.clone()).await;
        let ns_port = spawn_notification(sb_port).await;

        let sink = EventSink::new();
        let handle = start(
            ClientConfig {
                email: "me@example.com".into(),
                password: "secret".into(),
                server: "127.0.0.1".into(),
                port: ns_port,
                login_host: format!("http://127.0.0.1:{auth_port}"),
            },
            sink.clone(),
        );

        let mut signed_in = false;
        let mut message_id = None;
        loop {
            for event in sink.drain() {
                match event {
                    ClientEvent::SignedIn { .. } => signed_in = true,
                    ClientEvent::MessageSent { id, .. } => {
                        let expected = message_id.expect("the message id must be allocated before sending");
                        assert_eq!(id, expected, "the confirmation must refer to the sent message");
                        return;
                    }
                    ClientEvent::MessageFailed { message, .. } => {
                        panic!("A successful switchboard write must not be reported as failed: {message}")
                    }
                    _ => {}
                }
            }
            if signed_in && message_id.is_none() {
                handle.open_chat("friend@example.com");
                message_id = Some(handle.send_message("friend@example.com", "hello from linux".into()));
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("A written message must produce a MessageSent event with its id");
}

#[tokio::test]
async fn two_native_clients_exchange_messages_through_an_invited_switchboard() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (alice, alice_events, mut alice_ns) = contact_session_with("me@example.com", "").await;
        let (bob, bob_events, mut bob_ns) = contact_session_with("friend@example.com", "").await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        alice.send_message("friend@example.com", "Message from Alice".into());
        assert_eq!(read_line(&mut alice_ns).await, "XFR 10 SB");
        send(&mut alice_ns, &format!("XFR 10 SB 127.0.0.1:{port} CKI outgoing-token\r\n")).await;
        let (mut alice_sb, _) = listener.accept().await.unwrap();
        assert_eq!(read_line(&mut alice_sb).await, "USR 1 me@example.com outgoing-token", "Outgoing switchboards must authenticate directly");
        send(&mut alice_sb, "USR 1 OK me@example.com Alice\r\n").await;
        assert_eq!(read_line(&mut alice_sb).await, "CAL 2 friend@example.com");
        send(&mut alice_sb, "CAL 2 RINGING 42\r\n").await;
        send(&mut bob_ns, &format!("RNG 42 127.0.0.1:{port} CKI incoming-token me@example.com Alice\r\n")).await;
        let (mut bob_sb, _) = listener.accept().await.unwrap();
        assert_eq!(read_line(&mut bob_sb).await, "ANS 1 friend@example.com incoming-token 42", "Incoming switchboards must answer the invitation directly");
        send(&mut bob_sb, "IRO 1 1 1 me@example.com Alice\r\nANS 1 OK\r\n").await;
        send(&mut alice_sb, "JOI friend@example.com Bob\r\n").await;
        let header = read_line(&mut alice_sb).await;
        assert!(header.starts_with("MSG 3 N "));
        let payload = read_payload(&mut alice_sb, header.rsplit(' ').next().unwrap().parse().unwrap()).await;
        send(&mut bob_sb, &format!("MSG me@example.com Alice {}\r\n", payload.len())).await;
        bob_sb.write_all(&payload).await.unwrap();
        expect_message(&bob_events, "me@example.com", "Message from Alice").await;
        bob.send_message("me@example.com", "Reply from Bob".into());
        let header = read_line(&mut bob_sb).await;
        assert!(header.starts_with("MSG 3 N "));
        let payload = read_payload(&mut bob_sb, header.rsplit(' ').next().unwrap().parse().unwrap()).await;
        send(&mut alice_sb, &format!("MSG friend@example.com Bob {}\r\n", payload.len())).await;
        alice_sb.write_all(&payload).await.unwrap();
        expect_message(&alice_events, "friend@example.com", "Reply from Bob").await;
    }).await.expect("Both native accounts must receive each other's messages");
}

#[tokio::test]
async fn incoming_contact_requests_are_reported_without_automatic_approval() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (_client, sink, mut ns) = contact_session().await;
        send(&mut ns, "ADC 0 RL N=newfriend@escargot.chat F=New%20Friend\r\n").await;
        loop {
            if sink.drain().iter().any(|event| matches!(event,
                ClientEvent::ContactUpdated(info) if info.email == "newfriend@escargot.chat" && info.display_name == "New Friend" && info.lists & 8 != 0
            )) { break; }
            sleep(Duration::from_millis(5)).await;
        }
        assert!(tokio::time::timeout(Duration::from_millis(100), read_line(&mut ns)).await.is_err(), "Receiving a request must not send an automatic approval");
    }).await.expect("An incoming ADC RL must surface a contact request");
}

#[tokio::test]
async fn accepting_a_request_keeps_its_identity_and_stops_prompting() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (client, sink, mut ns) = contact_session().await;
        send(&mut ns, "ADC 0 RL N=newfriend@escargot.chat F=New%20Friend\r\n").await;
        client.add_contact("newfriend").unwrap();
        assert_eq!(read_line(&mut ns).await, "ADC 10 FL N=newfriend@escargot.chat F=newfriend%40escargot.chat");
        send(&mut ns, "ADC 10 FL N=newfriend@escargot.chat F=New%20Friend C=contact-uuid\r\n").await;
        assert_eq!(read_line(&mut ns).await, "ADC 11 AL N=newfriend@escargot.chat");
        send(&mut ns, "ADC 11 AL N=newfriend@escargot.chat\r\nADC 0 RL N=newfriend@escargot.chat F=New%20Friend\r\n").await;
        loop {
            if sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactUpdated(info)
                if info.email == "newfriend@escargot.chat" && info.lists == 11 && info.uuid.as_deref() == Some("contact-uuid") && !info.is_pending_request()
            )) { break; }
            sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("Accepting a request must retain the server identity and prevent another prompt");
}

#[tokio::test]
async fn pending_requests_survive_the_initial_contact_snapshot() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (_client, sink, _ns) = contact_session_with("me@example.com",
            "LST N=waiting@example.com F=Waiting C=request-uuid 24\r\nLST N=accepted@example.com F=Accepted C=accepted-uuid 11\r\n").await;
        loop {
            for event in sink.drain() {
                if let ClientEvent::ContactList { contacts, .. } = event {
                    let requests: Vec<_> = contacts.iter().filter(|contact| contact.is_pending_request()).map(|contact| contact.email.as_str()).collect();
                    assert_eq!(requests, ["waiting@example.com"], "Offline requests must be available after sign-in, without prompting for accepted contacts");
                    return;
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("Pending requests must be recovered when signing in");
}

#[tokio::test]
async fn removing_a_contact_waits_for_the_server_and_uses_its_uuid() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (client, sink, mut ns) = contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact(" Friend@EXAMPLE.com ").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        assert!(!sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoved { .. })), "Contacts must remain until the server confirms removal");
        send(&mut ns, "REM 10 FL 123 contact-uuid\r\n").await;
        assert_eq!(read_line(&mut ns).await, "REM 11 AL friend@example.com");
        send(&mut ns, "REM 11 AL 123 friend@example.com\r\n").await;
        loop {
            if sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoved { email } if email == "friend@example.com")) { break; }
            sleep(Duration::from_millis(5)).await;
        }
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut ns).await, "CHG 12 BSY");
    }).await.expect("Contact removal must be confirmed without disconnecting the account");
}

#[tokio::test]
async fn removing_a_contact_also_stops_status_sharing() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut ns) =
            contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        send(&mut ns, "REM 10 FL 123 contact-uuid\r\n").await;
        assert_eq!(
            read_line(&mut ns).await,
            "REM 11 AL friend@example.com",
            "Removing a contact must also stop sharing status with them"
        );
        assert!(
            !sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoved { .. })),
            "Removal must wait for the allow-list confirmation too"
        );
        send(&mut ns, "REM 11 AL 123 friend@example.com\r\n").await;
        loop {
            if sink.drain().iter().any(|event| matches!(
                event,
                ClientEvent::ContactRemoved { email } if email == "friend@example.com"
            )) {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Removing a contact must clean both lists before reporting success");
}

#[tokio::test]
async fn a_contact_can_be_added_again_after_removal_without_a_false_already_added_error() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut ns) =
            contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        send(&mut ns, "REM 10 FL 123 contact-uuid\r\n").await;
        assert_eq!(read_line(&mut ns).await, "REM 11 AL friend@example.com");
        send(&mut ns, "REM 11 AL 123 friend@example.com\r\n").await;
        loop {
            if sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoved { .. })) {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }

        client.add_contact("friend").unwrap();
        assert_eq!(read_line(&mut ns).await, "ADC 12 FL N=friend@escargot.chat F=friend%40escargot.chat");
        send(&mut ns, "ADC 12 FL N=friend@escargot.chat F=Friend C=contact-uuid\r\n").await;
        assert_eq!(read_line(&mut ns).await, "ADC 13 AL N=friend@escargot.chat");
        send(&mut ns, "215 13\r\n").await;
        loop {
            for event in sink.drain() {
                match event {
                    ClientEvent::ContactAdded { email } => {
                        assert_eq!(email, "friend@escargot.chat");
                        return;
                    }
                    ClientEvent::ContactAddFailed { message, .. } => {
                        panic!("An idempotent allow-list update must not fail the add: {message}")
                    }
                    _ => {}
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Re-adding a removed contact must remain possible");
}

#[tokio::test]
async fn an_existing_forward_entry_is_completed_with_the_allow_list() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (client, sink, mut ns) = contact_session().await;
        client.add_contact("friend").unwrap();
        assert_eq!(read_line(&mut ns).await, "ADC 10 FL N=friend@escargot.chat F=friend%40escargot.chat");
        send(&mut ns, "215 10\r\n").await;
        assert_eq!(
            read_line(&mut ns).await,
            "ADC 11 AL N=friend@escargot.chat",
            "A forward entry that already exists still needs its allow-list update"
        );
        send(&mut ns, "ADC 11 AL N=friend@escargot.chat\r\n").await;
        loop {
            if sink
                .drain()
                .iter()
                .any(|event| matches!(event, ClientEvent::ContactAdded { email } if email == "friend@escargot.chat"))
            {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("An already-present contact must be completed instead of reported as a failure");
}

#[tokio::test]
async fn a_rejected_removal_keeps_the_contact_available_for_retry() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (client, sink, mut ns) = contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        send(&mut ns, "500 10\r\n").await;
        loop {
            for event in sink.drain() {
                match event {
                    ClientEvent::ContactRemoveFailed { email, .. } if email == "friend@example.com" => {
                        client.remove_contact("friend@example.com").unwrap();
                        assert_eq!(read_line(&mut ns).await, "REM 11 FL contact-uuid");
                        return;
                    }
                    ClientEvent::ContactRemoved { .. } => panic!("A rejected removal must not remove the contact"),
                    _ => {},
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("A rejected removal must report its failure and allow retry");
}

#[tokio::test]
async fn removing_a_contact_without_a_server_identity_fails_without_a_wire_request() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (client, sink, mut ns) = contact_session_with("me@example.com", "LST N=friend@example.com F=Friend 1\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        loop {
            if sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoveFailed { email, message }
                if email == "friend@example.com" && message.contains("identity")
            )) { break; }
            sleep(Duration::from_millis(5)).await;
        }
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut ns).await, "CHG 10 BSY", "Missing identities must not send an invalid removal or disconnect");
    }).await.expect("Missing contact identities must produce a recoverable error");
}

#[tokio::test]
async fn a_removal_confirmation_for_another_contact_does_not_report_success() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (client, sink, mut ns) = contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        send(&mut ns, "REM 10 FL 123 another-contact-uuid\r\n").await;
        loop {
            for event in sink.drain() {
                match event {
                    ClientEvent::ContactRemoveFailed { email, .. } if email == "friend@example.com" => return,
                    ClientEvent::ContactRemoved { .. } => panic!("A confirmation for another contact must not remove this contact"),
                    _ => {},
                }
            }
            sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("A removal must confirm the same contact's server identity");
}

#[tokio::test]
async fn unanswered_removals_expire_and_late_replies_do_not_claim_success() {
    tokio::time::timeout(Duration::from_secs(23), async {
        let (client, sink, mut ns) = contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        loop {
            let mut expired = false;
            for event in sink.drain() {
                match event {
                    ClientEvent::ContactRemoveFailed { email, message } if email == "friend@example.com" => {
                        assert!(message.contains("did not confirm")); expired = true;
                    }
                    ClientEvent::ContactRemoved { .. } => panic!("An unanswered removal must not report success"),
                    _ => {},
                }
            }
            if expired { break; }
            sleep(Duration::from_millis(10)).await;
        }
        send(&mut ns, "REM 10 FL 123 contact-uuid\r\n").await;
        client.set_status(escargot_native::msnp::parsing::Status::Busy);
        assert_eq!(read_line(&mut ns).await, "CHG 11 BSY");
        sleep(Duration::from_millis(50)).await;
        assert!(!sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoved { .. })), "A late reply must not turn an expired operation into an unannounced success");
    }).await.expect("Unanswered removals must stop waiting without closing the session");
}

#[tokio::test]
async fn notification_payload_lengths_do_not_resolve_pending_contact_transactions() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (client, sink, mut ns) = contact_session_with("me@example.com", "LST N=friend@example.com F=Friend C=contact-uuid 3\r\n").await;
        client.remove_contact("friend@example.com").unwrap();
        assert_eq!(read_line(&mut ns).await, "REM 10 FL contact-uuid");
        send(&mut ns, "NOT 10\r\n0123456789").await;
        sleep(Duration::from_millis(50)).await;
        for event in sink.drain() {
            assert!(!matches!(event, ClientEvent::ContactRemoveFailed { .. } | ClientEvent::ContactRemoved { .. }), "A notification's byte count is not a transaction id: {event:?}");
        }
        send(&mut ns, "REM 10 FL 123 contact-uuid\r\n").await;
        assert_eq!(read_line(&mut ns).await, "REM 11 AL friend@example.com");
        send(&mut ns, "REM 11 AL 123 friend@example.com\r\n").await;
        loop {
            if sink.drain().iter().any(|event| matches!(event, ClientEvent::ContactRemoved { email } if email == "friend@example.com")) { return; }
            sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("Server notifications must leave the pending removal waiting for its real acknowledgement");
}
