//! Async MSNP11 client for Escargot.
//!
//! The protocol implementation follows the wire behaviour documented by the
//! Escargot/NINA server: TWN login through `msnmsgr.escargot.chat`, the
//! notification list (`SYN`/`LST`/`LSG`), presence (`ILN`/`NLN`/`FLN`/`UBX`),
//! and one switchboard per conversation.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::msnp::auth::PassportAuth;
use crate::msnp::framing::{Command, Framer};
use crate::msnp::mime::{build_im, build_nudge, build_typing, parse_payload};
use crate::msnp::parsing::{
    Status, parse_adc, parse_lsg, parse_lst, parse_presence, parse_ubx,
};

const CLIENT_ID: &str = "MSNMSGR 7.5.0324 msmsgs";

fn trace_line(direction: &str, line: &str) {
    if std::env::var_os("ESCARGOT_NATIVE_TRACE").is_none() {
        return;
    }

    let mut parts: Vec<&str> = line.trim_end().split(' ').collect();
    if parts.len() > 1 {
        if let Some(position) = parts.iter().position(|part| *part == "CKI") {
            if position + 1 < parts.len() {
                parts[position + 1] = "<redacted>";
            }
        }

        let twn = parts.iter().any(|part| *part == "TWN");
        if (parts[0] == "USR" || parts[0] == "ANS")
            && !twn
            && parts.get(2) != Some(&"OK")
            && parts.len() > 3
        {
            parts[3] = "<redacted>";
        }
    }

    eprintln!("[msnp {direction}] {}", parts.join(" "));
}
const PROTOCOL_VERSION: &str = "MSNP11";

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub email: String,
    pub password: String,
    pub server: String,
    pub port: u16,
    pub login_host: String,
}

#[derive(Clone, Debug)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct ContactInfo {
    pub email: String,
    pub display_name: String,
    pub status: Status,
    pub psm: Option<String>,
    pub uuid: Option<String>,
    pub lists: u32,
    pub group_ids: Vec<String>,
}

impl ContactInfo {
    pub fn is_pending_request(&self) -> bool {
        self.lists & (8 | 16) != 0 && self.lists & (2 | 4) == 0
    }
}

#[derive(Clone, Debug)]
pub enum ClientEvent {
    SignedIn { display_name: String },
    ContactList { groups: Vec<GroupInfo>, contacts: Vec<ContactInfo> },
    ContactUpdated(ContactInfo),
    ContactAdded { email: String },
    ContactRemoved { email: String },
    ContactRemoveWarning { email: String, message: String },
    ContactRemoveFailed { email: String, message: String },
    ContactAddFailed { email: String, message: String },
    ContactOffline { email: String },
    ContactPsm { email: String, psm: Option<String> },
    DisplayName(String),
    Message { peer: String, display_name: String, text: String },
    MessageSent { peer: String, id: u64 },
    MessageFailed { peer: String, id: u64, message: String },
    Typing { peer: String, active: bool },
    Nudge { peer: String },
    ChatReady { peer: String },
    ChatClosed { peer: String },
    ChatError { peer: String, message: String },
    Error(String),
    Disconnected(String),
}

#[derive(Clone)]
pub struct EventSink(Arc<Mutex<VecDeque<ClientEvent>>>);

impl Default for EventSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(VecDeque::new())))
    }

    pub fn push(&self, event: ClientEvent) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push_back(event);
        }
    }

    pub fn drain(&self) -> Vec<ClientEvent> {
        match self.0.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum ClientCommand {
    AddContact(String),
    RemoveContact(String),
    OpenChat(String),
    SendMessage { peer: String, text: String, id: u64 },
    SendTyping { peer: String, active: bool },
    SendNudge { peer: String },
    SetStatus(Status),
    LeaveChat(String),
}

#[derive(Clone)]
pub struct ClientHandle {
    tx: UnboundedSender<ClientCommand>,
    next_message_id: Arc<std::sync::atomic::AtomicU64>,
}

impl ClientHandle {
    pub fn add_contact(&self, input: &str) -> std::result::Result<(), String> {
        let email = crate::msnp::normalize_contact_account(input)?;
        self.tx.send(ClientCommand::AddContact(email))
            .map_err(|_| "You are disconnected. Sign in again.".to_string())
    }

    pub fn remove_contact(&self, input: &str) -> std::result::Result<(), String> {
        let email = crate::msnp::normalize_contact_account(input)?;
        self.tx.send(ClientCommand::RemoveContact(email))
            .map_err(|_| "You are disconnected. Sign in again.".to_string())
    }

    pub fn open_chat(&self, peer: &str) {
        let _ = self.tx.send(ClientCommand::OpenChat(peer.to_ascii_lowercase()));
    }

    /// Queues a message and returns its client-side id. The id is echoed back
    /// in `MessageSent` once the switchboard write succeeds, or in
    /// `MessageFailed` when the write fails.
    pub fn send_message(&self, peer: &str, text: String) -> u64 {
        let id = self
            .next_message_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = self.tx.send(ClientCommand::SendMessage {
            peer: peer.to_ascii_lowercase(),
            text,
            id,
        });
        id
    }

    pub fn send_typing(&self, peer: &str, active: bool) {
        let _ = self.tx.send(ClientCommand::SendTyping {
            peer: peer.to_ascii_lowercase(),
            active,
        });
    }

    pub fn send_nudge(&self, peer: &str) {
        let _ = self.tx.send(ClientCommand::SendNudge { peer: peer.to_ascii_lowercase() });
    }

    pub fn set_status(&self, status: Status) {
        let _ = self.tx.send(ClientCommand::SetStatus(status));
    }

    pub fn leave_chat(&self, peer: &str) {
        let _ = self.tx.send(ClientCommand::LeaveChat(peer.to_ascii_lowercase()));
    }
}

pub fn start(config: ClientConfig, sink: EventSink) -> ClientHandle {
    let (tx, rx) = unbounded_channel();
    let handle = ClientHandle {
        tx,
        next_message_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
    };
    let sink_for_thread = sink.clone();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                sink_for_thread.push(ClientEvent::Error(format!(
                    "Could not start the network runtime: {error}"
                )));
                return;
            }
        };

        let result = runtime.block_on(run(config, sink_for_thread.clone(), rx));
        if let Err(error) = result {
            sink_for_thread.push(ClientEvent::Error(error.to_string()));
            sink_for_thread.push(ClientEvent::Disconnected(error.to_string()));
        }
    });

    handle
}


struct PendingContactAdd {
    email: String,
    allow: bool,
    started: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RemovalStage {
    Forward,
    Allow,
}

struct PendingContactRemoval {
    email: String,
    uuid: String,
    stage: RemovalStage,
    started: Instant,
}

async fn run(
    config: ClientConfig,
    sink: EventSink,
    mut commands: UnboundedReceiver<ClientCommand>,
) -> Result<()> {
    let mut ns = Conn::connect(&config.server, config.port).await?;
    // Keep listening to the command channel while Passport and the notification
    // handshake run. Dropping the handle (Sign Out) closes the channel, which
    // cancels the pending sign-in instead of waiting for the HTTP timeout.
    let (groups, contacts, display_name) = {
        let sign_in = sign_in(&mut ns, &config, &sink);
        tokio::pin!(sign_in);
        loop {
            tokio::select! {
                result = &mut sign_in => break result?,
                command = commands.recv() => {
                    match command {
                        None => return Ok(()),
                        Some(ClientCommand::RemoveContact(email)) => sink.push(ClientEvent::ContactRemoveFailed {
                            email, message: "Wait for sign-in to finish before removing a contact.".into(),
                        }),
                        Some(ClientCommand::AddContact(email)) => sink.push(ClientEvent::ContactAddFailed {
                            email,
                            message: "Wait for sign-in to finish before adding a contact.".into(),
                        }),
                        _ => {},
                    }
                }
            }
        }
    };

    let mut known_contacts: HashMap<String, ContactInfo> =
        contacts.iter().cloned().map(|contact| (contact.email.clone(), contact)).collect();
    sink.push(ClientEvent::SignedIn { display_name });
    sink.push(ClientEvent::ContactList { groups, contacts });

    let mut switchboards: HashMap<String, UnboundedSender<SbCommand>> = HashMap::new();
    let mut xfr_targets: HashMap<u32, (String, Instant)> = HashMap::new();
    let mut queued: HashMap<String, Vec<ClientCommand>> = HashMap::new();
    let mut next_trid: u32 = 10;
    let mut contact_adds: HashMap<u32, PendingContactAdd> = HashMap::new();
    let mut contact_removals: HashMap<u32, PendingContactRemoval> = HashMap::new();
    let mut contact_deadlines = tokio::time::interval(Duration::from_secs(1));
    contact_deadlines.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let (sb_closed_tx, mut sb_closed_rx) = unbounded_channel::<String>();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(45));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = contact_deadlines.tick(), if !contact_adds.is_empty() || !contact_removals.is_empty() => {
                contact_removals.retain(|_, request| {
                    if request.started.elapsed() < Duration::from_secs(20) { return true; }
                    if request.stage == RemovalStage::Allow {
                        // The forward-list removal already succeeded; only status
                        // sharing is still pending. Report the partial result.
                        sink.push(ClientEvent::ContactRemoved { email: request.email.clone() });
                        sink.push(ClientEvent::ContactRemoveWarning {
                            email: request.email.clone(),
                            message: "Removed from your contacts, but Escargot did not confirm the status-sharing update. Refresh your contacts to check.".into(),
                        });
                    } else {
                        sink.push(ClientEvent::ContactRemoveFailed {
                            email: request.email.clone(),
                            message: "Escargot did not confirm removal. Check your list before trying again.".into(),
                        });
                    }
                    false
                });
                contact_adds.retain(|_, request| {
                    if request.started.elapsed() < Duration::from_secs(20) { return true; }
                    sink.push(ClientEvent::ContactAddFailed {
                        email: request.email.clone(),
                        message: "Escargot did not confirm the contact. Check your list before trying again.".into(),
                    });
                    false
                });
            }
            _ = heartbeat.tick() => {
                xfr_targets.retain(|_, (_, started)| started.elapsed() < Duration::from_secs(30));
                ns.send("PNG", &[], None).await?;
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    ClientCommand::RemoveContact(email) => {
                        let Some(uuid) = known_contacts.get(&email).and_then(|contact| contact.uuid.clone()) else {
                            sink.push(ClientEvent::ContactRemoveFailed {
                                email, message: "This contact's server identity is missing. Refresh your list and try again.".into(),
                            });
                            continue;
                        };
                        let trid = next_trid; next_trid += 1;
                        ns.send("REM", &[trid.to_string(), "FL".into(), uuid.clone()], None).await?;
                        contact_removals.insert(trid, PendingContactRemoval {
                            email,
                            uuid,
                            stage: RemovalStage::Forward,
                            started: Instant::now(),
                        });
                    }
                    ClientCommand::AddContact(email) => {
                        let trid = next_trid; next_trid += 1;
                        ns.send("ADC", &[
                            trid.to_string(), "FL".into(), format!("N={email}"),
                            format!("F={}", urlencoding::encode(&email)),
                        ], None).await?;
                        contact_adds.insert(trid, PendingContactAdd { email, allow: false, started: Instant::now() });
                    }
                    ClientCommand::OpenChat(peer) => {
                        open_conversation(&mut ns, &mut switchboards, &mut xfr_targets, &sink, &mut next_trid, &peer).await?;
                    }
                    ClientCommand::SendMessage { peer, text, id } => {
                        if let Some(tx) = switchboards.get(&peer) {
                            let _ = tx.send(SbCommand::SendText { text, id });
                        } else {
                            queued.entry(peer.clone()).or_default().push(ClientCommand::SendMessage { peer: peer.clone(), text, id });
                            open_conversation(&mut ns, &mut switchboards, &mut xfr_targets, &sink, &mut next_trid, &peer).await?;
                        }
                    }
                    ClientCommand::SendTyping { peer, active } => {
                        if let Some(tx) = switchboards.get(&peer) {
                            let _ = tx.send(SbCommand::Typing(active));
                        }
                    }
                    ClientCommand::SendNudge { peer } => {
                        if let Some(tx) = switchboards.get(&peer) {
                            let _ = tx.send(SbCommand::Nudge);
                        } else {
                            queued.entry(peer.clone()).or_default().push(ClientCommand::SendNudge { peer: peer.clone() });
                            open_conversation(&mut ns, &mut switchboards, &mut xfr_targets, &sink, &mut next_trid, &peer).await?;
                        }
                    }
                    ClientCommand::SetStatus(status) => {
                        let trid = next_trid; next_trid += 1;
                        ns.send("CHG", &[trid.to_string(), status.code().to_string()], None).await?;
                    }
                    ClientCommand::LeaveChat(peer) => {
                        if let Some(tx) = switchboards.remove(&peer) {
                            let _ = tx.send(SbCommand::Leave);
                        }
                        xfr_targets.retain(|_, (target, _)| target != &peer);
                    }
                }
            }
            command = ns.read_command(Duration::from_secs(120)) => {
                let command = command?;
                // Server pushes such as NOT and RNG also begin with a number.
                let contact_reply_id = if matches!(command.verb.as_str(), "ADC" | "REM")
                    || (command.verb.len() == 3 && command.verb.bytes().all(|byte| byte.is_ascii_digit()))
                { command.trid() } else { None };
                if let Some(request) = contact_reply_id.and_then(|trid| contact_removals.remove(&trid)) {
                    let params = command.params();
                    match request.stage {
                        RemovalStage::Forward => {
                            let confirmed = command.verb == "REM"
                                && params.first().map(String::as_str) == Some("FL")
                                && params.last() == Some(&request.uuid);
                            // 216 means the server already has no forward entry.
                            if confirmed || command.verb == "216" {
                                if let Some(contact) = known_contacts.get_mut(&request.email) {
                                    contact.lists &= !1;
                                }
                                let has_allow = known_contacts
                                    .get(&request.email)
                                    .is_some_and(|contact| contact.lists & 2 != 0);
                                if has_allow {
                                    let email = request.email.clone();
                                    let trid = next_trid; next_trid += 1;
                                    ns.send("REM", &[trid.to_string(), "AL".into(), email], None).await?;
                                    contact_removals.insert(trid, PendingContactRemoval {
                                        stage: RemovalStage::Allow,
                                        started: Instant::now(),
                                        ..request
                                    });
                                } else {
                                    sink.push(ClientEvent::ContactRemoved { email: request.email });
                                }
                            } else {
                                sink.push(ClientEvent::ContactRemoveFailed {
                                    email: request.email,
                                    message: format!("Escargot did not confirm removal (response {}). Check your list before trying again.", command.verb),
                                });
                            }
                        }
                        RemovalStage::Allow => {
                            let confirmed = command.verb == "REM"
                                && params.first().map(String::as_str) == Some("AL")
                                && params.last().is_some_and(|value| value.eq_ignore_ascii_case(&request.email));
                            // 216 means the allow list is already clear.
                            if confirmed || command.verb == "216" {
                                if let Some(contact) = known_contacts.get_mut(&request.email) {
                                    contact.lists &= !2;
                                }
                                sink.push(ClientEvent::ContactRemoved { email: request.email });
                            } else {
                                // The forward entry is gone; keep the result honest
                                // by removing the contact and warning about the
                                // status-sharing half.
                                sink.push(ClientEvent::ContactRemoved { email: request.email.clone() });
                                sink.push(ClientEvent::ContactRemoveWarning {
                                    email: request.email,
                                    message: format!("Removed from your contacts, but Escargot did not confirm the status-sharing update (response {}). Refresh your contacts to check.", command.verb),
                                });
                            }
                        }
                    }
                }
                if let Some(request) = contact_reply_id.and_then(|trid| contact_adds.remove(&trid)) {
                    let expected_list = if request.allow { "AL" } else { "FL" };
                    let confirmed = command.verb == "ADC" && parse_adc(command.params()).is_some_and(|entry| {
                        entry.list == expected_list && entry.email.eq_ignore_ascii_case(&request.email)
                    });
                    // 215 means the entry is already present. It is a successful
                    // idempotent result, not an error: this exact case used to make
                    // a re-added contact look like it had never been removed.
                    let already_present = command.verb == "215";
                    if confirmed || already_present {
                        if request.allow {
                            if let Some(contact) = known_contacts.get_mut(&request.email) {
                                contact.lists |= 2;
                            }
                            sink.push(ClientEvent::ContactAdded { email: request.email });
                        } else {
                            if let Some(contact) = known_contacts.get_mut(&request.email) {
                                contact.lists |= 1;
                            }
                            let trid = next_trid; next_trid += 1;
                            ns.send("ADC", &[trid.to_string(), "AL".into(), format!("N={}", request.email)], None).await?;
                            contact_adds.insert(trid, PendingContactAdd { allow: true, started: Instant::now(), ..request });
                        }
                    } else {
                        let reason = match command.verb.as_str() {
                            "205" | "208" => "This Escargot account does not exist. Check the username or email.".to_string(),
                            "210" => "Your contact list is full.".to_string(),
                            "201" => "Escargot rejected this address. Check the username or email.".to_string(),
                            code => format!("Escargot could not confirm the contact (response {code}). Try again."),
                        };
                        sink.push(ClientEvent::ContactAddFailed {
                            email: request.email,
                            message: if request.allow { format!("The contact was saved, but status sharing was not confirmed. {reason}") } else { reason },
                        });
                    }
                }
                handle_notification(&mut ns, &mut switchboards, &mut xfr_targets, &mut queued, &sink, &config, &mut next_trid, command, &sb_closed_tx, &mut known_contacts).await?;
            }
            closed = sb_closed_rx.recv() => {
                if let Some(peer) = closed {
                    switchboards.remove(&peer);
                    xfr_targets.retain(|_, (target, _)| target != &peer);
                }
            }
        }
    }

    Ok(())
}

async fn open_conversation(
    ns: &mut Conn,
    switchboards: &mut HashMap<String, UnboundedSender<SbCommand>>,
    xfr_targets: &mut HashMap<u32, (String, Instant)>,
    sink: &EventSink,
    next_trid: &mut u32,
    peer: &str,
) -> Result<()> {
    if switchboards.contains_key(peer) {
        sink.push(ClientEvent::ChatReady { peer: peer.to_string() });
        return Ok(());
    }
    if xfr_targets.values().any(|(target, _)| target == peer) {
        return Ok(());
    }

    let trid = *next_trid;
    *next_trid += 1;
    xfr_targets.insert(trid, (peer.to_string(), Instant::now()));
    ns.send("XFR", &[trid.to_string(), "SB".into()], None).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_notification(
    ns: &mut Conn,
    switchboards: &mut HashMap<String, UnboundedSender<SbCommand>>,
    xfr_targets: &mut HashMap<u32, (String, Instant)>,
    queued: &mut HashMap<String, Vec<ClientCommand>>,
    sink: &EventSink,
    config: &ClientConfig,
    next_trid: &mut u32,
    command: Command,
    sb_closed_tx: &UnboundedSender<String>,
    known_contacts: &mut HashMap<String, ContactInfo>,
) -> Result<()> {
    match command.verb.as_str() {
        "ILN" | "NLN" => {
            if let Some(presence) = parse_presence(command.params()) {
                let contact = known_contacts.entry(presence.email.clone()).or_insert(ContactInfo {
                    email: presence.email, display_name: String::new(), status: presence.status,
                    psm: None, uuid: None, lists: 1, group_ids: Vec::new(),
                });
                contact.status = presence.status;
                contact.display_name = presence.display_name;
                sink.push(ClientEvent::ContactUpdated(contact.clone()));
            }
        }
        "FLN" => {
            if let Some(email) = command.args.iter().find(|arg| arg.contains('@')) {
                sink.push(ClientEvent::ContactOffline { email: email.to_ascii_lowercase() });
            }
        }
        "UBX" => {
            if let Some(payload) = command.payload.as_deref() {
                if let Some((email, psm)) = parse_ubx(command.params(), payload) {
                    sink.push(ClientEvent::ContactPsm { email, psm });
                }
            }
        }
        "PRP" => {
            let params = command.params();
            if params.first().map(String::as_str) == Some("MFN") {
                if let Some(name) = params.get(1) {
                    sink.push(ClientEvent::DisplayName(
                        crate::msnp::parsing::percent_decode(name),
                    ));
                }
            }
        }
        "LST" => {
            if let Ok(entry) = parse_lst(command.params()) {
                let email = entry.email.clone();
                upsert_contact(known_contacts, ContactInfo {
                    email: entry.email, display_name: entry.display_name, status: Status::Offline,
                    psm: None, uuid: entry.uuid, lists: entry.lists, group_ids: entry.groups,
                });
                sink.push(ClientEvent::ContactUpdated(known_contacts[&email].clone()));
            }
        }
        "ADC" => {
            if let Some(adc) = parse_adc(command.params()) {
                let list = match adc.list.as_str() {
                    "FL" => 1, "AL" => 2, "BL" => 4, "RL" => 8, "PL" => 16,
                    _ => return Ok(()),
                };
                let contact = known_contacts.entry(adc.email.clone()).or_insert(ContactInfo {
                    email: adc.email, display_name: String::new(), status: Status::Offline,
                    psm: None, uuid: None, lists: 0, group_ids: Vec::new(),
                });
                contact.lists |= list;
                if let Some(name) = adc.display_name { contact.display_name = name; }
                if let Some(uuid) = adc.uuid { contact.uuid = Some(uuid); }
                sink.push(ClientEvent::ContactUpdated(contact.clone()));
            }
        }
        "REM" => {
            let params = command.params();
            if params.first().map(String::as_str) == Some("RL")
                && let Some(contact) = params.iter().find_map(|part| known_contacts.get(part))
            {
                let email = contact.email.clone();
                let contact = known_contacts.get_mut(&email).unwrap();
                contact.lists &= !(8 | 16);
                sink.push(ClientEvent::ContactUpdated(contact.clone()));
            }
        }
        "XFR" => {
            let params = command.params();
            if params.first().map(String::as_str) != Some("SB") {
                return Ok(());
            }
            let Some((target, _)) = command.trid().and_then(|trid| xfr_targets.remove(&trid)) else {
                return Ok(());
            };
            let Some(address) = params.get(1) else { return Ok(()) };
            let Some(host_port) = address.rsplit_once(':') else { return Ok(()) };
            let host = host_port.0.to_string();
            let port: u16 = host_port.1.parse().unwrap_or(1864);
            let Some(token) = params
                .iter()
                .position(|part| part == "CKI")
                .and_then(|index| params.get(index + 1))
                .cloned()
            else {
                sink.push(ClientEvent::ChatError {
                    peer: target.clone(),
                    message: "The switchboard response had no token.".into(),
                });
                return Ok(());
            };

            let (tx, rx) = unbounded_channel();
            switchboards.insert(target.clone(), tx.clone());
            let email = config.email.clone();
            let peer = target.clone();
            let sink_for_task = sink.clone();
            let closed = sb_closed_tx.clone();
            tokio::spawn(async move {
                if let Err(error) =
                    switchboard_outgoing(host, port, token, email, peer.clone(), sink_for_task.clone(), rx)
                        .await
                {
                    sink_for_task.push(ClientEvent::ChatError {
                        peer: peer.clone(),
                        message: error.to_string(),
                    });
                }
                let _ = closed.send(peer);
            });

            if let Some(messages) = queued.remove(&target) {
                for message in messages {
                    match message {
                        ClientCommand::SendMessage { text, id, .. } => {
                            let _ = tx.send(SbCommand::SendText { text, id });
                        }
                        ClientCommand::SendNudge { .. } => {
                            let _ = tx.send(SbCommand::Nudge);
                        }
                        _ => {}
                    }
                }
            }
        }
        "RNG" => {
            let params = &command.args;
            // RNG <session> <host:port> CKI <token> <email> <display>
            if params.len() < 6 {
                return Ok(());
            }
            let session = params[0].clone();
            let Some((host, port)) = params[1].rsplit_once(':') else {
                return Ok(());
            };
            let host = host.to_string();
            let port: u16 = port.parse().unwrap_or(1864);
            let token = params[3].clone();
            let peer = params[4].to_ascii_lowercase();

            let (tx, rx) = unbounded_channel();
            switchboards.insert(peer.clone(), tx);
            let email = config.email.clone();
            let sink_for_task = sink.clone();
            let peer_for_task = peer.clone();
            let closed = sb_closed_tx.clone();
            tokio::spawn(async move {
                if let Err(error) = switchboard_incoming(
                    host,
                    port,
                    token,
                    email,
                    session,
                    peer_for_task.clone(),
                    sink_for_task.clone(),
                    rx,
                )
                .await
                {
                    sink_for_task.push(ClientEvent::ChatError {
                        peer: peer_for_task.clone(),
                        message: error.to_string(),
                    });
                }
                let _ = closed.send(peer_for_task);
            });
        }
        "CHG" => {
            // Presence acknowledgement; nothing else to do.
        }
        _ => {}
    }

    let _ = next_trid;
    let _ = ns;
    Ok(())
}


/// Merges a list snapshot entry into the contact map without dropping presence
/// that arrived before the list (ILN can precede LST during sign-in).
fn upsert_contact(contacts: &mut HashMap<String, ContactInfo>, entry: ContactInfo) {
    if let Some(existing) = contacts.get_mut(&entry.email) {
        if !entry.display_name.is_empty() {
            existing.display_name = entry.display_name;
        }
        existing.uuid = entry.uuid.or_else(|| existing.uuid.clone());
        existing.lists = entry.lists;
        existing.group_ids = entry.group_ids;
    } else {
        contacts.insert(entry.email.clone(), entry);
    }
}

async fn sign_in(
    ns: &mut Conn,
    config: &ClientConfig,
    sink: &EventSink,
) -> Result<(Vec<GroupInfo>, Vec<ContactInfo>, String)> {
    ns.send("VER", &["0".into(), PROTOCOL_VERSION.into(), "CVR0".into()], None).await?;
    wait_for(ns, "VER", Duration::from_secs(20)).await?;

    ns.send(
        "CVR",
        &[
            "1".into(),
            "0x0409".into(),
            "winnt".into(),
            "5.1".into(),
            "i386".into(),
            CLIENT_ID.into(),
            config.email.clone(),
        ],
        None,
    )
    .await?;
    wait_for(ns, "CVR", Duration::from_secs(20)).await?;

    let trid = 2;
    ns.send(
        "USR",
        &[trid.to_string(), "TWN".into(), "I".into(), config.email.clone()],
        None,
    )
    .await?;

    let challenge = wait_for(ns, "USR", Duration::from_secs(20)).await?;
    if challenge.args.get(1).map(String::as_str) != Some("TWN") {
        bail!("server did not offer TWN authentication");
    }

    let email = config.email.clone();
    let password = config.password.clone();
    let login_host = config.login_host.clone();
    let token = tokio::task::spawn_blocking(move || {
        PassportAuth::new(login_host).fetch_ticket(&email, &password)
    })
    .await
    .map_err(|error| anyhow!("authentication task failed: {error}"))??;

    ns.send(
        "USR",
        &[
            "3".into(),
            "TWN".into(),
            "S".into(),
            format!("t={token}"),
        ],
        None,
    )
    .await?;

    let authenticated = wait_for_usr_ok(ns, Duration::from_secs(30)).await?;
    let _ = authenticated;

    ns.send("SYN", &["4".into(), "0".into()], None).await?;
    let _ = wait_for(ns, "SYN", Duration::from_secs(20)).await?;

    let mut groups: Vec<GroupInfo> = Vec::new();
    let mut contacts: HashMap<String, ContactInfo> = HashMap::new();
    let mut display_name = config
        .email
        .split('@')
        .next()
        .unwrap_or("Escargot User")
        .to_string();

    let chg_trid = 5u32;
    ns.send("CHG", &[chg_trid.to_string(), "NLN".into()], None).await?;

    // The list and profile arrive between SYN and the CHG acknowledgement.
    // Read until the acknowledgement, with a short grace period so a server
    // that acknowledges first still gets its list parsed by the main loop.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Ok(command) = ns.read_command(remaining).await else {
            break;
        };

        match command.verb.as_str() {
            "LSG" => {
                if let Some(group) = parse_lsg(command.params()) {
                    groups.retain(|existing| existing.id != group.id);
                    groups.push(GroupInfo { id: group.id, name: group.name });
                }
            }
            "LST" => {
                if let Ok(entry) = parse_lst(command.params()) {
                    upsert_contact(
                        &mut contacts,
                        ContactInfo {
                            email: entry.email,
                            display_name: entry.display_name,
                            status: Status::Offline,
                            psm: None,
                            uuid: entry.uuid,
                            lists: entry.lists,
                            group_ids: entry.groups,
                        },
                    );
                }
            }
            "ILN" | "NLN" => {
                if let Some(presence) = parse_presence(command.params()) {
                    let contact = contacts.entry(presence.email.clone()).or_insert(ContactInfo {
                        email: presence.email.clone(),
                        display_name: presence.display_name.clone(),
                        status: presence.status,
                        psm: None,
                        uuid: None,
                        lists: 1,
                        group_ids: Vec::new(),
                    });
                    contact.status = presence.status;
                    contact.display_name = presence.display_name;
                }
            }
            "PRP" => {
                let params = command.params();
                if params.first().map(String::as_str) == Some("MFN") {
                    if let Some(name) = params.get(1) {
                        display_name = crate::msnp::parsing::percent_decode(name);
                        sink.push(ClientEvent::DisplayName(display_name.clone()));
                    }
                }
            }
            "CHG" => {
                if command.trid() == Some(chg_trid) {
                    break;
                }
            }
            _ => {}
        }
    }

    Ok((groups, contacts.into_values().collect(), display_name))
}

async fn wait_for(ns: &mut Conn, verb: &str, timeout: Duration) -> Result<Command> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for {verb}");
        }
        let command = ns.read_command(remaining).await?;
        if command.verb == verb {
            return Ok(command);
        }
    }
}

async fn wait_for_usr_ok(ns: &mut Conn, timeout: Duration) -> Result<Command> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for the sign-in result");
        }
        let command = ns.read_command(remaining).await?;
        if command.verb == "USR" && command.args.get(1).map(String::as_str) == Some("OK") {
            return Ok(command);
        }
        if command.verb == "911" || command.verb == "924" {
            bail!("Escargot rejected the sign-in ({})", command.verb);
        }
    }
}

#[derive(Debug)]
enum SbCommand {
    SendText { text: String, id: u64 },
    Typing(bool),
    Nudge,
    Leave,
}

async fn switchboard_outgoing(
    host: String,
    port: u16,
    token: String,
    email: String,
    peer: String,
    sink: EventSink,
    rx: UnboundedReceiver<SbCommand>,
) -> Result<()> {
    let email_for_typing = email.clone();
    let mut conn = Conn::connect(&host, port).await?;
    conn.send("USR", &["1".into(), email, token], None).await?;
    wait_for(&mut conn, "USR", Duration::from_secs(15)).await?;

    conn.send("CAL", &["2".into(), peer.clone()], None).await?;
    wait_for_cal(&mut conn, &peer, 2).await?;

    switchboard_loop(conn, peer, email_for_typing, false, sink, rx).await
}

/// Waits for the CAL acknowledgement, surfacing numeric rejections such as
/// 217 (principal not online) instead of leaving the user with a 15-second
/// read timeout and a generic "connection timed out" line.
async fn wait_for_cal(conn: &mut Conn, peer: &str, trid: u32) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("Escargot did not answer the conversation invite. Try again.");
        }
        let command = match conn.read_command(remaining).await {
            Ok(command) => command,
            Err(error) if error.to_string().contains("timed out") => {
                bail!("Escargot did not answer the conversation invite. Try again.")
            }
            Err(error) => return Err(error),
        };

        match command.verb.as_str() {
            "CAL" if command.trid() == Some(trid) => {
                if command.args.iter().any(|arg| arg == "RINGING") {
                    return Ok(());
                }
                bail!("Escargot rejected the conversation invite.");
            }
            "217" => bail!("{peer} is offline right now. Try again when they sign in."),
            "205" | "208" => bail!("This account does not exist on Escargot."),
            "201" => bail!("Escargot rejected the conversation invite."),
            "911" | "924" => bail!("The conversation server rejected the session. Sign in again."),
            code if code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_digit()) => {
                bail!("Escargot could not start the conversation (response {code}).")
            }
            _ => {}
        }
    }
}

async fn switchboard_incoming(
    host: String,
    port: u16,
    token: String,
    email: String,
    session: String,
    peer: String,
    sink: EventSink,
    rx: UnboundedReceiver<SbCommand>,
) -> Result<()> {
    let email_for_typing = email.clone();
    let mut conn = Conn::connect(&host, port).await?;
    conn.send("ANS", &["1".into(), email, token, session], None).await?;
    wait_for(&mut conn, "ANS", Duration::from_secs(15)).await?;
    sink.push(ClientEvent::ChatReady { peer: peer.clone() });
    switchboard_loop(conn, peer, email_for_typing, true, sink, rx).await
}

async fn switchboard_loop(
    mut conn: Conn,
    peer: String,
    email: String,
    already_joined: bool,
    sink: EventSink,
    mut rx: UnboundedReceiver<SbCommand>,
) -> Result<()> {
    let mut joined = already_joined;
    let mut queued: Vec<SbCommand> = Vec::new();
    let mut trid: u32 = 3;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(45));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if joined {
                    conn.send("PNG", &[], None).await?;
                }
            }
            command = rx.recv() => {
                let Some(command) = command else { break };
                match command {
                    SbCommand::Leave => {
                        let _ = conn.send("OUT", &[], None).await;
                        sink.push(ClientEvent::ChatClosed { peer: peer.clone() });
                        break;
                    }
                    SbCommand::SendText { text, id } => {
                        if !joined {
                            queued.push(SbCommand::SendText { text, id });
                            continue;
                        }
                        if let Err(error) = send_text(&mut conn, &mut trid, &text).await {
                            sink.push(ClientEvent::MessageFailed {
                                peer: peer.clone(),
                                id,
                                message: error.to_string(),
                            });
                            return Err(error);
                        }
                        sink.push(ClientEvent::MessageSent { peer: peer.clone(), id });
                    }
                    SbCommand::Typing(active) => {
                        if joined && active {
                            let payload = build_typing(&email);
                            conn.send("MSG", &[trid.to_string(), "U".into()], Some(&payload)).await?;
                            trid += 1;
                        }
                    }
                    SbCommand::Nudge => {
                        if !joined {
                            queued.push(SbCommand::Nudge);
                            continue;
                        }
                        let payload = build_nudge();
                        conn.send("MSG", &[trid.to_string(), "N".into()], Some(&payload)).await?;
                        trid += 1;
                    }
                }
            }
            command = conn.read_command(Duration::from_secs(3600)) => {
                let command = command?;
                match command.verb.as_str() {
                    "JOI" => {
                        if !joined {
                            joined = true;
                            sink.push(ClientEvent::ChatReady { peer: peer.clone() });
                            for pending in queued.drain(..) {
                                match pending {
                                    SbCommand::SendText { text, id } => {
                                        if let Err(error) = send_text(&mut conn, &mut trid, &text).await {
                                            sink.push(ClientEvent::MessageFailed {
                                                peer: peer.clone(),
                                                id,
                                                message: error.to_string(),
                                            });
                                            return Err(error);
                                        }
                                        sink.push(ClientEvent::MessageSent { peer: peer.clone(), id });
                                    }
                                    SbCommand::Nudge => {
                                        let payload = build_nudge();
                                        conn.send("MSG", &[trid.to_string(), "N".into()], Some(&payload)).await?;
                                        trid += 1;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "MSG" => {
                        if let Some(payload) = command.payload.as_deref() {
                            let message = parse_payload(payload);
                            if message.typing_user.is_some() {
                                sink.push(ClientEvent::Typing { peer: peer.clone(), active: true });
                            } else if message.datacast_id == Some(1) {
                                sink.push(ClientEvent::Nudge { peer: peer.clone() });
                            } else if !message.text.is_empty() {
                                let display_name = command
                                    .args
                                    .get(1)
                                    .map(|name| crate::msnp::parsing::percent_decode(name))
                                    .unwrap_or_else(|| peer.clone());
                                sink.push(ClientEvent::Message {
                                    peer: peer.clone(),
                                    display_name,
                                    text: message.text,
                                });
                                sink.push(ClientEvent::Typing { peer: peer.clone(), active: false });
                            }
                        }
                    }
                    "BYE" => {
                        sink.push(ClientEvent::ChatClosed { peer: peer.clone() });
                        break;
                    }
                    "NAK" => {
                        sink.push(ClientEvent::ChatError {
                            peer: peer.clone(),
                            message: "The message was rejected by the switchboard.".into(),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

async fn send_text(conn: &mut Conn, trid: &mut u32, text: &str) -> Result<()> {
    let payload = build_im(text);
    conn.send("MSG", &[trid.to_string(), "N".into()], Some(&payload)).await?;
    *trid += 1;
    Ok(())
}

struct Conn {
    stream: TcpStream,
    framer: Framer,
    queue: VecDeque<Command>,
}

impl Conn {
    async fn connect(host: &str, port: u16) -> Result<Self> {
        let stream = tokio::time::timeout(
            Duration::from_secs(20),
            TcpStream::connect((host, port)),
        )
        .await
        .map_err(|_| anyhow!("connection to {host}:{port} timed out"))??;
        stream.set_nodelay(true).ok();
        Ok(Self { stream, framer: Framer::default(), queue: VecDeque::new() })
    }

    async fn read_command(&mut self, timeout: Duration) -> Result<Command> {
        loop {
            if let Some(command) = self.queue.pop_front() {
                let mut line = command.verb.clone();
                for arg in &command.args {
                    line.push(' ');
                    line.push_str(arg);
                }
                if let Some(payload) = &command.payload {
                    line.push_str(&format!(" <{} bytes>", payload.len()));
                }
                trace_line("<<", &line);
                return Ok(command);
            }

            let mut buffer = [0u8; 8192];
            let read = tokio::time::timeout(timeout, self.stream.read(&mut buffer))
                .await
                .map_err(|_| anyhow!("the connection timed out"))??;
            if read == 0 {
                bail!("the server closed the connection");
            }
            self.queue.extend(self.framer.feed(&buffer[..read]));
        }
    }

    async fn send(&mut self, verb: &str, args: &[String], payload: Option<&[u8]>) -> Result<()> {
        let mut line = String::new();
        line.push_str(verb);
        for arg in args {
            line.push(' ');
            line.push_str(arg);
        }
        if let Some(payload) = payload {
            line.push(' ');
            line.push_str(&payload.len().to_string());
        }
        line.push_str("\r\n");
        trace_line(">>", &line);
        self.stream.write_all(line.as_bytes()).await?;
        if let Some(payload) = payload {
            self.stream.write_all(payload).await?;
        }
        self.stream.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    fn contact(email: &str, status: Status, groups: Vec<String>) -> ContactInfo {
        ContactInfo {
            email: email.into(),
            display_name: "Friend".into(),
            status,
            psm: None,
            uuid: Some("uuid".into()),
            lists: 1,
            group_ids: groups,
        }
    }

    #[test]
    fn the_event_queue_keeps_events_until_drained() {
        let sink = EventSink::new();
        for index in 0..600 {
            sink.push(ClientEvent::DisplayName(format!("name-{index}")));
        }
        assert_eq!(sink.drain().len(), 600);
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn a_list_snapshot_does_not_downgrade_online_presence() {
        let mut contacts = HashMap::new();
        upsert_contact(
            &mut contacts,
            contact("friend@example.com", Status::Online, vec![]),
        );
        upsert_contact(
            &mut contacts,
            contact("friend@example.com", Status::Offline, vec!["group-1".into()]),
        );
        let merged = contacts.get("friend@example.com").unwrap();
        assert_eq!(merged.status, Status::Online);
        assert_eq!(merged.group_ids, vec!["group-1".to_string()]);
    }
}
