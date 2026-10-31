use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::Local;
use gpui::{Context, SharedString};

use crate::msnp::normalize_account;
use crate::config;
use crate::ui::theme;
use crate::history::History;
use crate::msnp::client::{ClientConfig, ClientEvent, ClientHandle, ContactInfo, EventSink, start};
use crate::msnp::parsing::Status;

#[derive(Clone, Debug)]
pub struct Contact {
    pub email: String,
    pub display_name: String,
    pub status: Status,
    pub psm: Option<String>,
    pub group: String,
    /// The contact is on our forward list, but the server has not shown a
    /// reverse-list entry yet, so they have not added us back.
    pub pending_outgoing: bool,
}

impl Contact {
    pub fn label(&self) -> String {
        friendly_name(&self.display_name, &self.email)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sending,
    Sent,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    /// Client-side id used to match `MessageSent` / `MessageFailed` events.
    /// Historical and incoming messages use 0.
    pub id: u64,
    pub from_me: bool,
    pub author: String,
    pub text: String,
    pub system: bool,
    pub at: String,
    pub delivery: Delivery,
}

#[derive(Clone, Debug)]
pub struct GroupState {
    pub name: String,
    pub open: bool,
}

pub struct Store {
    pub email: String,
    pub display_name: String,
    pub status: Status,
    pub wallpaper: Option<String>,
    pub interface_wallpaper: Option<String>,
    pub night: bool,
    pub single_window: bool,
    pub active_chat: Option<String>,
    pub groups: Vec<GroupState>,
    pub contacts: Vec<Contact>,
    pub contact_requests: Vec<ContactInfo>,
    pub recent: Vec<crate::history::RecentPeer>,
    pub removing_contacts: HashSet<String>,
    pub messages: HashMap<String, Vec<ChatMessage>>,
    pub typing: HashMap<String, bool>,
    pub unread: HashMap<String, usize>,
    pub signed_in: bool,
    pub login_error: Option<SharedString>,
    pub demo: bool,
    pub client: Option<ClientHandle>,
    pub events: EventSink,
    pub connection_error: Option<SharedString>,
    pub connecting: bool,
    pub refreshing: bool,
    pub adding_contact: Option<String>,
    pub contact_add_target: Option<String>,
    pub contact_add_result: Option<Result<String, String>>,
    password: Option<String>,
    reconnect_at: Option<Instant>,
    reconnect_attempts: u8,
    auth_failed: bool,
    pub open_chats: HashSet<String>,
    pub chat_ready: HashSet<String>,
    pub chat_errors: HashMap<String, String>,
    history: Option<History>,
    loaded_history: HashSet<String>,
    typing_deadline: HashMap<String, Instant>,
    pending_messages: HashMap<u64, String>,
    delivery_deadlines: HashMap<u64, Instant>,
    next_local_message_id: u64,
    pump_started: bool,
}

impl Store {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let settings = config::load();
        theme::set_night(settings.night);
        crate::ui::scale::set_percent(settings.ui_zoom);
        let wallpaper = match settings.wallpaper.as_deref() {
            Some(name) if !theme::wallpaper_allowed(name) => {
                theme::default_wallpaper().map(str::to_string)
            }
            _ => settings.wallpaper,
        };
        Self {
            email: String::new(),
            display_name: "Escargot User".into(),
            status: Status::Online,
            wallpaper,
            interface_wallpaper: settings.interface_wallpaper.filter(|name| theme::wallpaper_allowed(name)),
            night: settings.night,
            single_window: settings.single_window,
            active_chat: None,
            groups: Vec::new(),
            contacts: Vec::new(),
            contact_requests: Vec::new(),
            recent: Vec::new(),
            removing_contacts: HashSet::new(),
            messages: HashMap::new(),
            typing: HashMap::new(),
            unread: HashMap::new(),
            signed_in: false,
            login_error: None,
            demo: false,
            client: None,
            events: EventSink::new(),
            connection_error: None,
            connecting: false,
            refreshing: false,
            adding_contact: None,
            contact_add_target: None,
            contact_add_result: None,
            password: None,
            reconnect_at: None,
            reconnect_attempts: 0,
            auth_failed: false,
            open_chats: HashSet::new(),
            chat_ready: HashSet::new(),
            chat_errors: HashMap::new(),
            history: None,
            loaded_history: HashSet::new(),
            typing_deadline: HashMap::new(),
            pending_messages: HashMap::new(),
            delivery_deadlines: HashMap::new(),
            next_local_message_id: 1_000_000_000,
            pump_started: false,
        }
    }

    pub fn sign_in(
        &mut self,
        email: &str,
        password: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        self.login_error = None;
        self.adding_contact = None;
        self.contact_add_target = None;
        self.contact_add_result = None;
        self.contact_requests.clear();
        self.removing_contacts.clear();
        self.pending_messages.clear();
        self.delivery_deadlines.clear();
        self.chat_errors.clear();
        self.refreshing = false;
        let email = normalize_account(email);
        if email.is_empty() {
            self.login_error = Some("Enter your Escargot username.".into());
            return false;
        }
        if password.is_empty() {
            self.login_error = Some("Enter your password.".into());
            return false;
        }

        self.email = email;
        let local = self
            .email
            .split('@')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or("escargot user");
        let mut chars = local.chars();
        self.display_name = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => "Escargot User".to_string(),
        };
        self.connection_error = None;
        self.auth_failed = false;
        self.reconnect_attempts = 0;
        self.reconnect_at = None;
        self.password = if self.demo { None } else { Some(password.to_string()) };

        if self.demo {
            self.signed_in = true;
            self.connecting = false;
            self.seed_demo();
        } else {
            self.signed_in = false;
            self.connecting = true;
            self.contacts.clear();
            self.groups.clear();
            self.messages.clear();
            self.unread.clear();
            self.loaded_history.clear();
            self.history = History::open(&self.email).ok();
            self.recent = self
                .history
                .as_ref()
                .and_then(|history| history.recent_peers(50).ok())
                .unwrap_or_default();
            self.events = EventSink::new();
            self.client = Some(start(self.client_config(), self.events.clone()));
        }

        self.start_event_pump(cx);
        true
    }

    fn client_config(&self) -> ClientConfig {
        ClientConfig {
            email: self.email.clone(),
            password: self.password.clone().unwrap_or_default(),
            server: "m1.escargot.log1p.xyz".into(),
            port: 1863,
            login_host: "https://msnmsgr.escargot.chat".into(),
        }
    }

    fn reconnect(&mut self, cx: &mut Context<Self>) {
        if self.password.is_none() || self.auth_failed {
            return;
        }
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        if self.reconnect_attempts > 3 {
            self.connecting = false;
            self.signed_in = false;
            self.connection_error = Some("Connection lost. Sign in again.".into());
            cx.notify();
            return;
        }

        self.events = EventSink::new();
        self.client = Some(start(self.client_config(), self.events.clone()));
        self.connecting = true;
        self.connection_error = None;
        cx.notify();
    }

    fn start_event_pump(&mut self, cx: &mut Context<Self>) {
        if self.pump_started {
            return;
        }
        self.pump_started = true;

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let alive = this
                    .update(cx, |store, cx| {
                        if store.drain_events(cx) {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn drain_events(&mut self, cx: &mut Context<Self>) -> bool {
        let events = self.events.drain();
        let mut changed = !events.is_empty();
        for event in events {
            self.apply_event(event);
        }

        if self.reconnect_at.is_some_and(|at| at <= Instant::now()) {
            self.reconnect_at = None;
            self.reconnect(cx);
            changed = true;
        }

        let now = Instant::now();
        let expired: Vec<String> = self
            .typing_deadline
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(peer, _)| peer.clone())
            .collect();
        for peer in expired {
            self.typing_deadline.remove(&peer);
            self.typing.insert(peer, false);
            changed = true;
        }

        let expired_deliveries: Vec<u64> = self
            .delivery_deadlines
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in expired_deliveries {
            if let Some(peer) = self.pending_messages.get(&id).cloned() {
                self.fail_message(
                    &peer,
                    id,
                    "Escargot did not confirm the message. Check your connection and retry.",
                );
                changed = true;
            }
            self.delivery_deadlines.remove(&id);
        }
        changed
    }

    fn apply_event(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::SignedIn { display_name } => {
                if !display_name.is_empty() {
                    self.display_name = display_name;
                }
                self.signed_in = true;
                self.connecting = false;
                self.reconnect_attempts = 0;
                self.reconnect_at = None;
                self.connection_error = None;
            }
            ClientEvent::DisplayName(name) => {
                if !name.is_empty() {
                    self.display_name = name;
                }
            }
            ClientEvent::ContactList { groups, contacts } => {
                self.refreshing = false;
                self.contact_requests = incoming_requests(&self.email, &contacts);
                let by_id: HashMap<String, String> = groups
                    .iter()
                    .map(|group| (group.id.clone(), group.name.clone()))
                    .collect();

                self.groups = groups
                    .into_iter()
                    .map(|group| GroupState { name: group.name, open: true })
                    .collect();

                self.contacts = contacts
                    .into_iter()
                    .filter(|contact| !contact.email.eq_ignore_ascii_case(&self.email))
                    .filter(|contact| contact.lists & 1 != 0 || contact.lists == 0)
                    .map(|contact| {
                        let group = contact
                            .group_ids
                            .iter()
                            .find_map(|id| by_id.get(id).cloned())
                            .unwrap_or_else(|| "Other Contacts".into());
                        Contact {
                            email: contact.email,
                            display_name: contact.display_name,
                            status: contact.status,
                            pending_outgoing: is_pending_outgoing(contact.lists)
                                && contact.status == Status::Offline,
                            psm: contact.psm,
                            group,
                        }
                    })
                    .collect();

                for contact in &self.contacts {
                    if !self.groups.iter().any(|group| group.name == contact.group) {
                        self.groups.push(GroupState {
                            name: contact.group.clone(),
                            open: true,
                        });
                    }
                }
                if self.groups.is_empty() {
                    self.groups.push(GroupState { name: "Other Contacts".into(), open: true });
                }
            }
            ClientEvent::ContactUpdated(info) => {
                self.contact_requests.retain(|request| request.email != info.email);
                if info.email.eq_ignore_ascii_case(&self.email) {
                    // The server can report our own principal as a pending
                    // reverse entry; never show it as a contact or request.
                    return;
                }
                if is_incoming_request(&self.email, &info)
                    && !self
                        .contact_requests
                        .iter()
                        .any(|request| request.email.eq_ignore_ascii_case(&info.email))
                {
                    self.contact_requests.push(info.clone());
                }
                if info.lists & 1 == 0 { return; }
                if let Some(contact) = self
                    .contacts
                    .iter_mut()
                    .find(|contact| contact.email.eq_ignore_ascii_case(&info.email))
                {
                    contact.display_name = if info.display_name.is_empty() {
                        contact.display_name.clone()
                    } else {
                        info.display_name
                    };
                    contact.pending_outgoing = is_pending_outgoing(info.lists)
                        && info.status == Status::Offline
                        && contact.status == Status::Offline;
                    if info.status != Status::Offline || contact.status == Status::Offline {
                        contact.status = info.status;
                    }
                    if info.psm.is_some() {
                        contact.psm = info.psm;
                    }
                } else {
                    let contact = Contact {
                        email: info.email,
                        display_name: info.display_name,
                        status: info.status,
                        psm: info.psm,
                        group: "Other Contacts".into(),
                        pending_outgoing: is_pending_outgoing(info.lists)
                            && info.status == Status::Offline,
                    };
                    if !self.groups.iter().any(|group| group.name == contact.group) {
                        self.groups.push(GroupState {
                            name: contact.group.clone(),
                            open: true,
                        });
                    }
                    self.contacts.push(contact);
                }
            }
            ClientEvent::ContactRemoved { email } => {
                self.removing_contacts.remove(&email);
                self.contacts.retain(|contact| contact.email != email);
                self.add_system_message(&email, "Contact removed. Your conversation history was kept.");
            }
            ClientEvent::ContactRemoveWarning { email, message } => {
                self.add_system_message(&email, message);
            }
            ClientEvent::ContactRemoveFailed { email, message } => {
                self.removing_contacts.remove(&email);
                self.add_system_message(&email, message);
            }
            ClientEvent::ContactAdded { email } => {
                self.contact_requests.retain(|request| request.email != email);
                if let Some(contact) = self
                    .contacts
                    .iter_mut()
                    .find(|contact| contact.email.eq_ignore_ascii_case(&email))
                {
                    contact.pending_outgoing = true;
                }
                if self.adding_contact.as_deref() == Some(email.as_str()) {
                    self.adding_contact = None;
                    self.contact_add_result = Some(Ok(email));
                }
            }
            ClientEvent::ContactAddFailed { email, message } => {
                if self.adding_contact.as_deref() == Some(email.as_str()) {
                    self.adding_contact = None;
                    self.contact_add_result = Some(Err(message));
                }
            }
            ClientEvent::ContactOffline { email } => {
                if let Some(contact) = self
                    .contacts
                    .iter_mut()
                    .find(|contact| contact.email.eq_ignore_ascii_case(&email))
                {
                    contact.status = Status::Offline;
                }
            }
            ClientEvent::ContactPsm { email, psm } => {
                if let Some(contact) = self
                    .contacts
                    .iter_mut()
                    .find(|contact| contact.email.eq_ignore_ascii_case(&email))
                {
                    contact.psm = psm;
                }
            }
            ClientEvent::Message { peer, display_name, text } => {
                let peer = peer.to_ascii_lowercase();
                if let Some(contact) = self
                    .contacts
                    .iter_mut()
                    .find(|contact| contact.email.eq_ignore_ascii_case(&peer))
                {
                    if !display_name.is_empty() {
                        contact.display_name = display_name.clone();
                    }
                }
                self.typing.insert(peer.clone(), false);
                self.typing_deadline.remove(&peer);
                let message = ChatMessage {
                    id: 0,
                    from_me: false,
                    author: display_name,
                    text,
                    system: false,
                    at: now(),
                    delivery: Delivery::Sent,
                };
                if let Some(history) = &self.history {
                    let _ = history.append(&peer, &message);
                }
                let preview = message.text.clone();
                let at = message.at.clone();
                self.messages.entry(peer.clone()).or_default().push(message);
                self.touch_recent(&peer, &preview, false, &at);
                let reading = if self.single_window {
                    self.active_chat.as_deref() == Some(peer.as_str())
                } else {
                    self.open_chats.contains(&peer)
                };
                if !reading {
                    *self.unread.entry(peer).or_insert(0) += 1;
                }
            }
            ClientEvent::MessageSent { peer, id } => {
                let peer = peer.to_ascii_lowercase();
                let sent = self.messages.get_mut(&peer).and_then(|messages| {
                    messages
                        .iter_mut()
                        .find(|message| message.id == id && message.delivery == Delivery::Sending)
                }).map(|message| {
                    message.delivery = Delivery::Sent;
                    message.clone()
                });
                if let Some(message) = sent {
                    if let Some(history) = &self.history {
                        let _ = history.append(&peer, &message);
                    }
                    self.touch_recent(&peer, &message.text, true, &message.at);
                }
                self.pending_messages.remove(&id);
                self.delivery_deadlines.remove(&id);
                self.chat_errors.remove(&peer);
            }
            ClientEvent::MessageFailed { peer, id, message } => {
                let peer = peer.to_ascii_lowercase();
                self.fail_message(&peer, id, &message);
                self.chat_errors.insert(peer, message);
            }
            ClientEvent::Typing { peer, active } => {
                let peer = peer.to_ascii_lowercase();
                self.typing.insert(peer.clone(), active);
                if active {
                    self.typing_deadline
                        .insert(peer, Instant::now() + Duration::from_secs(6));
                } else {
                    self.typing_deadline.remove(&peer);
                }
            }
            ClientEvent::Nudge { peer } => {
                let name = self.contact_name(&peer);
                self.add_system_message(&peer, format!("{name} sent you a nudge!"));
            }
            ClientEvent::ChatReady { peer } => {
                let peer = peer.to_ascii_lowercase();
                self.chat_ready.insert(peer.clone());
                self.chat_errors.remove(&peer);
                self.typing.insert(peer, false);
            }
            ClientEvent::ChatClosed { peer } => {
                let peer = peer.to_ascii_lowercase();
                self.fail_pending_for_peer(
                    &peer,
                    "The conversation ended before Escargot confirmed the message.",
                );
                self.chat_ready.remove(&peer);
            }
            ClientEvent::ChatError { peer, message } => {
                let peer = peer.to_ascii_lowercase();
                self.chat_errors.insert(peer.clone(), message.clone());
                self.fail_pending_for_peer(&peer, &message);
            }
            ClientEvent::Error(message) => {
                self.fail_pending_messages(
                    "Connection lost before Escargot confirmed the message.",
                );
                for email in std::mem::take(&mut self.removing_contacts) {
                    self.add_system_message(&email, "Contact removal was not confirmed. Sign in and check your list.");
                }
                if self.adding_contact.take().is_some() {
                    self.contact_add_result = Some(Err("Connection lost. Sign in again before adding the contact.".into()));
                }
                self.signed_in = false;
                self.connecting = false;
                self.refreshing = false;
                if message.to_ascii_lowercase().contains("incorrect email or password")
                    || message.to_ascii_lowercase().contains("rejected the sign-in")
                {
                    self.auth_failed = true;
                }
                self.connection_error = Some(message.into());
            }
            ClientEvent::Disconnected(message) => {
                self.fail_pending_messages(
                    "Connection lost before Escargot confirmed the message.",
                );
                for email in std::mem::take(&mut self.removing_contacts) {
                    self.add_system_message(&email, "Contact removal was not confirmed. Sign in and check your list.");
                }
                if self.adding_contact.take().is_some() {
                    self.contact_add_result = Some(Err("Connection lost. Sign in again before adding the contact.".into()));
                }
                self.signed_in = false;
                self.connecting = false;
                self.refreshing = false;
                if self.auth_failed || self.password.is_none() {
                    if !message.is_empty() {
                        self.connection_error = Some(message.into());
                    }
                } else if self.reconnect_attempts < 3 {
                    self.reconnect_at = Some(Instant::now() + Duration::from_secs(2));
                    self.connecting = true;
                    self.connection_error = None;
                } else {
                    self.connection_error = Some("Connection lost. Sign in again.".into());
                }
            }
        }
    }

    pub fn seed_demo(&mut self) {
        self.groups = vec![
            GroupState { name: "Friends".into(), open: true },
            GroupState { name: "Family".into(), open: true },
            GroupState { name: "Other Contacts".into(), open: true },
        ];

        self.contacts = vec![
            Contact {
                email: "alex@escargot.chat".into(),
                display_name: "Alex".into(),
                status: Status::Online,
                psm: Some("listening to Frou Frou".into()),
                group: "Friends".into(),
                pending_outgoing: false,
            },
            Contact {
                email: "marina@escargot.chat".into(),
                display_name: "Marina".into(),
                status: Status::Away,
                psm: None,
                group: "Friends".into(),
                pending_outgoing: false,
            },
            Contact {
                email: "pixel@escargot.chat".into(),
                display_name: "Pixel".into(),
                status: Status::Busy,
                psm: Some("do not disturb".into()),
                group: "Friends".into(),
                pending_outgoing: false,
            },
            Contact {
                email: "sam@escargot.chat".into(),
                display_name: "Sam".into(),
                status: Status::Offline,
                psm: None,
                group: "Family".into(),
                pending_outgoing: false,
            },
            Contact {
                email: "mum@escargot.chat".into(),
                display_name: "Mum".into(),
                status: Status::Offline,
                psm: None,
                group: "Family".into(),
                pending_outgoing: false,
            },
        ];

        // Demo-only stress fixture for long lists and long names. Never used by
        // a real sign-in; it keeps the manual UI probes reproducible without
        // touching an account.
        if std::env::var_os("ESCARGOT_DEMO_MANY").is_some() {
            for index in 0..40 {
                let status = match index % 4 {
                    0 => Status::Online,
                    1 => Status::Away,
                    2 => Status::Busy,
                    _ => Status::Offline,
                };
                self.contacts.push(Contact {
                    email: format!("demo{index:02}@escargot.chat"),
                    display_name: format!(
                        "Demo Contact {index:02} with a deliberately long display name"
                    ),
                    status,
                    psm: (index % 2 == 0).then(|| "a long personal message that should truncate".into()),
                    group: if index % 5 == 0 { "Other Contacts".into() } else { "Friends".into() },
                    pending_outgoing: false,
                });
            }
        }

        self.messages.clear();
        self.messages.insert(
            "alex@escargot.chat".into(),
            vec![
                message(false, "Alex", "hey! you're actually running this on linux now?"),
                message(true, "You", "native. no wine, no vm."),
                message(false, "Alex", "ok that's genuinely impressive :D"),
                message(false, "Alex", "send me a nudge when you're in :P"),
            ],
        );
        self.messages.insert(
            "marina@escargot.chat".into(),
            vec![message(false, "Marina", "see you at eight?")],
        );
        self.unread.insert("alex@escargot.chat".into(), 2);
        self.recent.clear();
        self.touch_recent(
            "marina@escargot.chat",
            "see you at eight?",
            false,
            "20:00",
        );
        self.touch_recent(
            "alex@escargot.chat",
            "send me a nudge when you're in :P",
            false,
            "20:01",
        );
        if std::env::var_os("ESCARGOT_DEMO_REQUESTS").is_some() {
            self.apply_event(ClientEvent::ContactUpdated(ContactInfo {
                email: "taylor@escargot.chat".into(), display_name: "Taylor".into(),
                status: Status::Offline, psm: None, uuid: None, lists: 8, group_ids: Vec::new(),
            }));
        }
    }

    pub fn prepare_add_form(&mut self) {
        if self.adding_contact.is_none() {
            self.contact_add_target = None;
            self.contact_add_result = None;
        }
    }

    pub fn add_contact(&mut self, input: &str, cx: &mut Context<Self>) {
        if self.adding_contact.is_some() { return; }
        self.contact_add_result = None;
        self.contact_add_target = None;
        let result = crate::msnp::normalize_contact_account(input).and_then(|email| {
            if !self.signed_in { return Err("Sign in before adding a contact.".into()); }
            if email.eq_ignore_ascii_case(&self.email) { return Err("Enter the other person's account, not your own.".into()); }
            self.contact_add_target = Some(email.clone());
            if self.demo {
                if self.contact(&email).is_none() {
                    self.apply_event(ClientEvent::ContactUpdated(ContactInfo {
                        display_name: email.split('@').next().unwrap_or(&email).to_string(),
                        email: email.clone(), status: Status::Offline, psm: None, uuid: None,
                        lists: 3, group_ids: Vec::new(),
                    }));
                }
                self.contact_requests.retain(|request| request.email != email);
                self.contact_add_result = Some(Ok(email));
                return Ok(());
            }
            let client = self.client.as_ref().ok_or_else(|| "You are disconnected. Sign in again.".to_string())?;
            client.add_contact(&email)?;
            self.adding_contact = Some(email);
            Ok(())
        });
        if let Err(message) = result { self.contact_add_result = Some(Err(message)); }
        cx.notify();
    }

    pub fn remove_contact(&mut self, input: &str, cx: &mut Context<Self>) {
        let result = crate::msnp::normalize_contact_account(input).and_then(|email| {
            if self.removing_contacts.contains(&email) { return Ok(()); }
            if !self.signed_in { return Err("Sign in before removing a contact.".to_string()); }
            if self.demo {
                self.apply_event(ClientEvent::ContactRemoved { email });
                return Ok(());
            }
            let client = self.client.as_ref().ok_or_else(|| "You are disconnected. Sign in again.".to_string())?;
            client.remove_contact(&email)?;
            self.removing_contacts.insert(email);
            Ok(())
        });
        if let Err(message) = result { self.add_system_message(input, message); }
        cx.notify();
    }

    pub fn contact(&self, email: &str) -> Option<&Contact> {
        self.contacts
            .iter()
            .find(|contact| contact.email.eq_ignore_ascii_case(email))
    }

    pub fn contact_name(&self, email: &str) -> String {
        self.contact(email)
            .map(Contact::label)
            .unwrap_or_else(|| friendly_name("", email))
    }

    pub fn own_label(&self) -> String {
        friendly_name(&self.display_name, &self.email)
    }

    pub fn messages_for(&self, email: &str) -> &[ChatMessage] {
        self.messages
            .get(&email.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn send_message(&mut self, email: &str, text: String) {
        let email = email.to_ascii_lowercase();
        if text.trim().is_empty() {
            return;
        }
        let local_id = self.next_local_message_id;
        self.next_local_message_id = self.next_local_message_id.saturating_add(1);
        let name = self.contact_name(&email);
        let offline = self
            .contact(&email)
            .is_some_and(|contact| contact.status == Status::Offline && !self.demo);

        let (id, delivery) = if self.demo {
            (local_id, Delivery::Sent)
        } else if offline {
            (
                local_id,
                Delivery::Failed(format!(
                    "{name} is offline. Try again when they sign in."
                )),
            )
        } else if !self.signed_in {
            (
                local_id,
                Delivery::Failed("You are offline. Reconnect and try again.".into()),
            )
        } else if let Some(client) = &self.client {
            (client.send_message(&email, text.clone()), Delivery::Sending)
        } else {
            (
                local_id,
                Delivery::Failed("You are disconnected. Sign in again.".into()),
            )
        };

        let message = ChatMessage {
            id,
            from_me: true,
            author: "You".into(),
            text,
            system: false,
            at: now(),
            delivery: delivery.clone(),
        };
        if delivery == Delivery::Sent {
            if let Some(history) = &self.history {
                let _ = history.append(&email, &message);
            }
        }
        if delivery == Delivery::Sending {
            self.pending_messages.insert(id, email.clone());
            self.delivery_deadlines
                .insert(id, Instant::now() + Duration::from_secs(15));
        }
        let preview = message.text.clone();
        let at = message.at.clone();
        let sent = delivery == Delivery::Sent;
        self.messages.entry(email.clone()).or_default().push(message);
        if sent {
            self.touch_recent(&email, &preview, true, &at);
        }
        self.typing.insert(email.clone(), false);
        self.typing_deadline.remove(&email);
    }

    /// Re-sends a failed outgoing message. The old row is updated in place so
    /// the transcript does not gain a duplicate.
    pub fn retry_message(&mut self, email: &str, id: u64) {
        let email = email.to_ascii_lowercase();
        let Some(text) = self
            .messages
            .get(&email)
            .and_then(|messages| messages.iter().find(|message| message.id == id && message.from_me))
            .map(|message| message.text.clone())
        else {
            return;
        };
        let name = self.contact_name(&email);
        let offline = self
            .contact(&email)
            .is_some_and(|contact| contact.status == Status::Offline && !self.demo);

        let (new_id, delivery) = if self.demo {
            (id, Delivery::Sent)
        } else if offline {
            (
                id,
                Delivery::Failed(format!(
                    "{name} is offline. Try again when they sign in."
                )),
            )
        } else if !self.signed_in {
            (id, Delivery::Failed("You are offline. Reconnect and try again.".into()))
        } else if let Some(client) = &self.client {
            (client.send_message(&email, text), Delivery::Sending)
        } else {
            (id, Delivery::Failed("You are disconnected. Sign in again.".into()))
        };

        if let Some(message) = self
            .messages
            .get_mut(&email)
            .and_then(|messages| messages.iter_mut().find(|message| message.id == id && message.from_me))
        {
            message.id = new_id;
            message.delivery = delivery.clone();
            message.at = now();
        }
        if delivery == Delivery::Sending {
            self.pending_messages.insert(new_id, email.clone());
            self.delivery_deadlines
                .insert(new_id, Instant::now() + Duration::from_secs(15));
        } else {
            self.pending_messages.remove(&id);
            self.delivery_deadlines.remove(&id);
        }
    }

    fn fail_message(&mut self, email: &str, id: u64, reason: &str) {
        if let Some(message) = self
            .messages
            .get_mut(&email.to_ascii_lowercase())
            .and_then(|messages| messages.iter_mut().find(|message| message.id == id))
        {
            if message.delivery == Delivery::Sending {
                message.delivery = Delivery::Failed(reason.to_string());
            }
        }
        self.pending_messages.remove(&id);
        self.delivery_deadlines.remove(&id);
    }

    fn fail_pending_for_peer(&mut self, email: &str, reason: &str) {
        let ids: Vec<u64> = self
            .pending_messages
            .iter()
            .filter(|(_, peer)| peer.eq_ignore_ascii_case(email))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.fail_message(email, id, reason);
        }
    }

    fn fail_pending_messages(&mut self, reason: &str) {
        let pending: Vec<(u64, String)> = self
            .pending_messages
            .iter()
            .map(|(id, peer)| (*id, peer.clone()))
            .collect();
        for (id, peer) in pending {
            self.fail_message(&peer, id, reason);
        }
        self.pending_messages.clear();
        self.delivery_deadlines.clear();
    }

    pub fn send_nudge(&mut self, email: &str) {
        if let Some(client) = &self.client {
            client.send_nudge(email);
        }
        self.add_system_message(email, "You sent a nudge!");
    }

    pub fn open_chat(&mut self, email: &str) {
        let email = email.to_ascii_lowercase();
        self.open_chats.insert(email.clone());
        self.active_chat = Some(email.clone());
        self.unread.remove(&email);
        if !self.loaded_history.contains(&email) {
            if let Some(history) = &self.history {
                if let Ok(stored) = history.load(&email, 500) {
                    self.messages.insert(email.clone(), stored);
                }
            }
            self.loaded_history.insert(email.clone());
        }
        if let Some(client) = &self.client {
            client.open_chat(&email);
        }
    }

    pub fn typing(&mut self, email: &str, active: bool) {
        if let Some(client) = &self.client {
            client.send_typing(email, active);
        }
    }

    pub fn add_system_message(&mut self, email: &str, text: impl Into<String>) {
        let email = email.to_ascii_lowercase();
        let text = text.into();
        let message = ChatMessage {
            id: 0,
            from_me: false,
            author: String::new(),
            text: text.clone(),
            system: true,
            at: now(),
            delivery: Delivery::Sent,
        };
        let messages = self.messages.entry(email).or_default();
        // System notices are ephemeral. Keeping them out of SQLite prevents
        // every reconnect from piling up "conversation ended" lines, and the
        // duplicate guard keeps one failed operation from spamming the view.
        push_system_notice(messages, message);
    }

    fn touch_recent(&mut self, email: &str, preview: &str, from_me: bool, at: &str) {
        let email = email.to_ascii_lowercase();
        self.recent.retain(|recent| !recent.peer.eq_ignore_ascii_case(&email));
        self.recent.insert(
            0,
            crate::history::RecentPeer {
                peer: email,
                preview: preview.to_string(),
                at: at.to_string(),
                from_me,
            },
        );
        self.recent.truncate(50);
    }

    pub fn clear_chat_error(&mut self, email: &str) {
        self.chat_errors.remove(&email.to_ascii_lowercase());
    }

    pub fn set_chat_error(&mut self, email: &str, message: impl Into<String>) {
        self.chat_errors
            .insert(email.to_ascii_lowercase(), message.into());
    }

    pub fn has_failed_messages(&self, email: &str) -> bool {
        self.messages
            .get(&email.to_ascii_lowercase())
            .is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| matches!(message.delivery, Delivery::Failed(_)))
            })
    }

    pub fn retry_failed_messages(&mut self, email: &str) {
        let email = email.to_ascii_lowercase();
        let ids: Vec<u64> = self
            .messages
            .get(&email)
            .map(|messages| {
                messages
                    .iter()
                    .filter(|message| {
                        message.from_me && matches!(message.delivery, Delivery::Failed(_))
                    })
                    .map(|message| message.id)
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            self.retry_message(&email, id);
        }
    }

    /// Explains why the composer cannot send right now. `None` means the
    /// protocol should be allowed to try.
    pub fn send_block_reason(&self, email: &str) -> Option<String> {
        if self.demo {
            return None;
        }
        if !self.signed_in {
            return Some("You are offline. Reconnect to send messages.".into());
        }
        let contact = self.contact(email)?;
        let name = contact.label();
        if contact.pending_outgoing {
            return Some(format!(
                "Waiting for {name} to add you back before you can chat."
            ));
        }
        if contact.status == Status::Offline {
            return Some(format!("{name} is offline. Try again when they sign in."));
        }
        None
    }

    pub fn dismiss_request(&mut self, email: &str) {
        self.contact_requests
            .retain(|request| !request.email.eq_ignore_ascii_case(email));
    }

    pub fn request_add_error<'a>(&'a self, email: &str) -> Option<&'a str> {
        if self
            .contact_add_target
            .as_deref()
            .is_some_and(|target| target.eq_ignore_ascii_case(email))
        {
            if let Some(Err(message)) = &self.contact_add_result {
                return Some(message);
            }
        }
        None
    }

    pub fn mark_read(&mut self, email: &str) {
        self.unread.remove(&email.to_ascii_lowercase());
    }

    pub fn unread_for(&self, email: &str) -> usize {
        self.unread
            .get(&email.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    }

    pub fn online_counts(&self, group: &str) -> (usize, usize) {
        let contacts = self.contacts.iter().filter(|contact| contact.group == group);
        let total = contacts.clone().count();
        let online = contacts
            .filter(|contact| contact.status != Status::Offline)
            .count();
        (online, total)
    }

    pub fn toggle_night(&mut self, cx: &mut Context<Self>) {
        self.night = !self.night;
        theme::set_night(self.night);
        let mut settings = config::load();
        settings.night = self.night;
        if self
            .wallpaper
            .as_deref()
            .is_some_and(|name| !theme::wallpaper_allowed(name))
        {
            self.wallpaper = theme::default_wallpaper().map(str::to_string);
            settings.wallpaper = self.wallpaper.clone();
        }
        let _ = config::save(&settings);
        cx.notify();
    }

    pub fn set_active_chat(&mut self, email: Option<String>, cx: &mut Context<Self>) {
        if let Some(email) = &email {
            self.unread.remove(email);
        }
        self.active_chat = email.map(|email| email.to_ascii_lowercase());
        cx.notify();
    }

    pub fn close_chat(&mut self, email: &str, cx: &mut Context<Self>) {
        let email = email.to_ascii_lowercase();
        self.open_chats.remove(&email);
        if self.active_chat.as_deref() == Some(email.as_str()) {
            self.active_chat = None;
        }
        if let Some(client) = &self.client {
            client.leave_chat(&email);
        }
        cx.notify();
    }

    pub fn set_single_window(&mut self, single: bool, cx: &mut Context<Self>) {
        self.single_window = single;
        let mut settings = config::load();
        settings.single_window = single;
        let _ = config::save(&settings);
        cx.notify();
    }

    pub fn set_interface_wallpaper(&mut self, wallpaper: Option<String>, cx: &mut Context<Self>) {
        if wallpaper.as_deref().is_some_and(|name| !theme::wallpaper_allowed(name)) { return; }
        self.interface_wallpaper = wallpaper.clone();
        let mut settings = config::load();
        settings.interface_wallpaper = wallpaper;
        let _ = config::save(&settings);
        cx.notify();
    }

    pub fn set_wallpaper(&mut self, wallpaper: Option<String>, cx: &mut Context<Self>) {
        if wallpaper.as_deref().is_some_and(|name| !theme::wallpaper_allowed(name)) { return; }
        self.wallpaper = wallpaper.clone();
        let mut settings = config::load();
        settings.wallpaper = wallpaper;
        let _ = config::save(&settings);
        cx.notify();
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status;
        if let Some(client) = &self.client {
            client.set_status(status);
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.signed_in || self.password.is_none() || self.refreshing {
            return;
        }
        self.refreshing = true;
        self.fail_pending_messages("Contacts refreshed before Escargot confirmed the message.");
        self.chat_ready.clear();
        self.client = None;
        self.events = EventSink::new();
        self.reconnect_attempts = 0;
        self.connection_error = None;
        self.reconnect(cx);
    }

    /// Recovery from the connection banner. Unlike opening the login window,
    /// this keeps conversations and drafts alive.
    pub fn can_retry_connection(&self) -> bool {
        self.password.is_some() && !self.auth_failed
    }

    pub fn retry_connection(&mut self, cx: &mut Context<Self>) {
        if self.password.is_none() || self.auth_failed {
            return;
        }
        self.fail_pending_messages("Reconnecting before Escargot confirmed the message.");
        self.chat_ready.clear();
        self.client = None;
        self.events = EventSink::new();
        self.reconnect_attempts = 0;
        self.reconnect_at = None;
        self.connection_error = None;
        self.connecting = true;
        self.refreshing = false;
        self.reconnect(cx);
    }

    pub fn sign_out(&mut self) {
        self.adding_contact = None;
        self.contact_add_target = None;
        self.contact_add_result = None;
        self.contact_requests.clear();
        self.recent.clear();
        self.removing_contacts.clear();
        self.refreshing = false;
        self.client = None;
        self.events = EventSink::new();
        self.password = None;
        self.signed_in = false;
        self.connecting = false;
        self.connection_error = None;
        self.login_error = None;
        self.reconnect_at = None;
        self.reconnect_attempts = 0;
        self.auth_failed = false;
        self.contacts.clear();
        self.groups.clear();
        self.messages.clear();
        self.unread.clear();
        self.typing.clear();
        self.typing_deadline.clear();
        self.chat_ready.clear();
        self.chat_errors.clear();
        self.pending_messages.clear();
        self.delivery_deadlines.clear();
        self.open_chats.clear();
        self.active_chat = None;
        self.history = None;
        self.loaded_history.clear();
        self.display_name = "Escargot User".into();
        self.status = Status::Online;
    }
}

/// A contact on the forward list without a reverse-list entry has not added
/// us back yet. `PL` is the server's forged pending bit in list snapshots.
pub fn is_pending_outgoing(lists: u32) -> bool {
    lists & 1 != 0 && lists & (8 | 16) == 0
}

/// Requests must never include the signed-in account itself. Escargot can
/// expose our own principal through the pending list, which previously
/// produced a card asking "Mafuzyk wants to add you" to Mafuzyk.
pub fn is_incoming_request(own_email: &str, info: &crate::msnp::client::ContactInfo) -> bool {
    info.is_pending_request() && !info.email.eq_ignore_ascii_case(own_email)
}

/// Cuts a long display name at a stable character budget and keeps an explicit
/// ellipsis. Proportional fonts can still clip the glyph, but the user sees that
/// the name was shortened instead of a hard cut.
pub fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(max).collect();
    shortened.push('…');
    shortened
}

pub fn incoming_requests(
    own_email: &str,
    contacts: &[crate::msnp::client::ContactInfo],
) -> Vec<crate::msnp::client::ContactInfo> {
    let mut seen = HashSet::new();
    contacts
        .iter()
        .filter(|info| is_incoming_request(own_email, info))
        .filter(|info| seen.insert(info.email.to_ascii_lowercase()))
        .cloned()
        .collect()
}

/// Prefer the server's friendly name, but never show an address as the primary
/// label when it is identical to the account. "jane@escargot.chat" becomes
/// "Jane" and the full address stays available as secondary text.
pub fn friendly_name(name: &str, email: &str) -> String {
    let name = name.trim();
    if !name.is_empty() && !name.eq_ignore_ascii_case(email) {
        return name.to_string();
    }
    let local = email.split('@').next().unwrap_or(email).trim();
    let local = if local.is_empty() { email } else { local };
    let mut chars = local.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn message(from_me: bool, author: &str, text: &str) -> ChatMessage {
    ChatMessage {
        id: 0,
        from_me,
        author: author.into(),
        text: text.into(),
        system: false,
        at: now(),
        delivery: Delivery::Sent,
    }
}

fn push_system_notice(messages: &mut Vec<ChatMessage>, message: ChatMessage) {
    if messages
        .last()
        .is_some_and(|last| last.system && last.text == message.text)
    {
        return;
    }
    messages.push(message);
}

fn now() -> String {
    Local::now().format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_list_bits_are_what_make_a_contact_mutual() {
        assert!(is_pending_outgoing(1), "FL without RL is an outgoing request");
        assert!(is_pending_outgoing(3), "FL+AL without RL is still pending");
        assert!(
            !is_pending_outgoing(9) && !is_pending_outgoing(11),
            "A reverse-list entry means the other person added us back"
        );
        assert!(!is_pending_outgoing(8), "RL alone is an incoming request");
    }

    #[test]
    fn addresses_are_not_used_as_primary_names() {
        assert_eq!(
            friendly_name("jane@escargot.chat", "jane@escargot.chat"),
            "Jane"
        );
        assert_eq!(friendly_name("Alex", "alex@escargot.chat"), "Alex");
        assert_eq!(friendly_name("", "mum@escargot.chat"), "Mum");
        assert_eq!(friendly_name("  ", "sam@escargot.chat"), "Sam");
    }

    #[test]
    fn own_account_is_never_an_incoming_request() {
        let request = |email: &str, lists: u32| crate::msnp::client::ContactInfo {
            email: email.into(),
            display_name: String::new(),
            status: Status::Offline,
            psm: None,
            uuid: None,
            lists,
            group_ids: Vec::new(),
        };
        let contacts = vec![
            request("jane@escargot.chat", 8),
            request("friend@escargot.chat", 8),
            request("friend@escargot.chat", 8),
            request("outgoing@escargot.chat", 1),
        ];
        let requests = incoming_requests("JANE@escargot.chat", &contacts);
        assert_eq!(
            requests.len(),
            1,
            "The signed-in account must never request itself, and duplicates must collapse"
        );
        assert_eq!(requests[0].email, "friend@escargot.chat");
    }

    #[test]
    fn long_labels_get_an_explicit_ellipsis() {
        assert_eq!(elide("Alex", 26), "Alex");
        let long = "Demo Contact 01 with a deliberately long display name";
        let cut = elide(long, 26);
        assert_eq!(cut.chars().count(), 27);
        assert!(cut.ends_with('…'));
        assert!(long.starts_with(&cut[..cut.len() - '…'.len_utf8()]));
    }

    #[test]
    fn consecutive_system_notices_are_deduplicated() {
        let mut messages = Vec::new();
        let notice = |text: &str| ChatMessage {
            id: 0,
            from_me: false,
            author: String::new(),
            text: text.into(),
            system: true,
            at: now(),
            delivery: Delivery::Sent,
        };
        push_system_notice(&mut messages, notice("The conversation ended."));
        push_system_notice(&mut messages, notice("The conversation ended."));
        assert_eq!(messages.len(), 1, "A repeated system notice must not stack");
        push_system_notice(&mut messages, notice("Contact removed."));
        assert_eq!(messages.len(), 2, "A different notice still appears");
    }
}
