//! Connectivity probe for Escargot.
//!
//! Without credentials it checks the notification server handshake and the
//! Passport-compatible endpoint. With `ESCARGOT_EMAIL` and
//! `ESCARGOT_PASSWORD` set it also acquires a real ticket and completes the
//! MSNP sign-in step. Secrets are never printed.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use escargot_native::msnp::auth::{AuthError, PassportAuth};
use escargot_native::msnp::normalize_account;

const SERVER: &str = "m1.escargot.log1p.xyz:1863";
const NEXUS: &str = "https://msnmsgr.escargot.chat";

fn main() {
    let email = std::env::var("ESCARGOT_EMAIL").ok().map(|email| normalize_account(&email));
    let password = std::env::var("ESCARGOT_PASSWORD").ok();

    println!("Nexus: {NEXUS}/rdr/pprdr.asp");
    match PassportAuth::new(NEXUS).fetch_ticket("probe@example.com", "wrong-password") {
        Err(AuthError::InvalidCredentials) => {
            println!("auth endpoint: reachable (bogus credentials rejected)")
        }
        Err(error) => println!("auth endpoint: {error}"),
        Ok(_) => println!("auth endpoint: unexpected success with bogus credentials"),
    }

    let mut stream = match TcpStream::connect(SERVER) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("TCP {SERVER}: {error}");
            std::process::exit(1);
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(8))).ok();

    let probe_email = email.clone().unwrap_or_else(|| "probe@example.com".into());
    if email.is_some() {
        println!("account: {probe_email}");
    }
    send(&mut stream, "VER 0 MSNP11 CVR0");
    show("VER", read_reply(&mut stream));
    send(
        &mut stream,
        &format!("CVR 1 0x0409 winnt 5.1 i386 MSNMSGR 7.5.0324 msmsgs {probe_email}"),
    );
    show("CVR", read_reply(&mut stream));
    send(&mut stream, &format!("USR 2 TWN I {probe_email}"));
    show("USR challenge", read_reply(&mut stream));

    let (Some(email), Some(password)) = (email, password) else {
        println!("set ESCARGOT_EMAIL and ESCARGOT_PASSWORD to test a real sign-in");
        return;
    };

    println!("requesting a Passport ticket...");
    let ticket = match PassportAuth::new(NEXUS).fetch_ticket(&email, &password) {
        Ok(ticket) => ticket,
        Err(error) => {
            eprintln!("sign-in failed: {error}");
            std::process::exit(2);
        }
    };
    println!("ticket acquired ({} characters)", ticket.len());

    send(
        &mut stream,
        &format!("USR 3 TWN S t={ticket}"),
    );
    let reply = read_reply(&mut stream);
    if reply.contains(" OK ") || reply.contains(" OK\r\n") {
        println!("USR authenticated: OK");
    } else {
        println!("USR result: {}", reply.lines().next().unwrap_or("<empty>"));
    }
}

fn send(stream: &mut TcpStream, line: &str) {
    let _ = stream.write_all(format!("{line}\r\n").as_bytes());
    let _ = stream.flush();
    std::thread::sleep(Duration::from_millis(500));
}

fn read_reply(stream: &mut TcpStream) -> String {
    let mut buffer = [0u8; 4096];
    let mut out = String::new();
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                out.push_str(&String::from_utf8_lossy(&buffer[..read]));
                if out.contains("\r\n") {
                    break;
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error) => {
                out.push_str(&format!("<read error: {error}>"));
                break;
            }
        }
    }
    out.trim().to_string()
}

fn show(label: &str, reply: String) {
    if reply.is_empty() {
        println!("{label}: <no immediate reply>");
    } else {
        for line in reply.lines() {
            println!("{label}: {line}");
        }
    }
}
