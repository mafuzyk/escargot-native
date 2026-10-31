use std::fs;

use escargot_native::history::History;
use escargot_native::state::{ChatMessage, Delivery};

fn message(text: &str, from_me: bool) -> ChatMessage {
    ChatMessage {
        id: 0,
        from_me,
        author: if from_me { "You".into() } else { "Friend".into() },
        text: text.into(),
        system: false,
        at: "20:00".into(),
        delivery: Delivery::Sent,
    }
}

fn system_message(text: &str) -> ChatMessage {
    ChatMessage {
        id: 0,
        from_me: false,
        author: String::new(),
        text: text.into(),
        system: true,
        at: "20:01".into(),
        delivery: Delivery::Sent,
    }
}

#[test]
fn appends_and_reloads_conversation_history() {
    let path = std::env::temp_dir().join(format!(
        "escargot-history-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let history = History::open_at(&path, "me@example.com").unwrap();
        history.append("friend@example.com", &message("first", false)).unwrap();
        history.append("friend@example.com", &message("second", true)).unwrap();
        history.append("other@example.com", &message("elsewhere", false)).unwrap();
    }

    let history = History::open_at(&path, "ME@example.com").unwrap();
    let loaded = history.load("friend@example.com", 100).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].text, "first");
    assert_eq!(loaded[1].text, "second");
    assert!(loaded[1].from_me);

    let other = history.load("other@example.com", 100).unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].text, "elsewhere");

    let _ = fs::remove_file(path);
}

#[test]
fn recent_threads_survive_contact_loss_and_ignore_system_noise() {
    let path = std::env::temp_dir().join(format!(
        "escargot-recent-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let history = History::open_at(&path, "me@example.com").unwrap();
        history.append("friend@example.com", &message("older", false)).unwrap();
        history.append("other@example.com", &message("elsewhere", false)).unwrap();
        history.append("friend@example.com", &message("latest", true)).unwrap();
        history
            .append(
                "friend@example.com",
                &system_message("Contact removed. Your conversation history was kept."),
            )
            .unwrap();
    }

    let history = History::open_at(&path, "me@example.com").unwrap();
    let recent = history.recent_peers(10).unwrap();
    assert_eq!(recent.len(), 2, "Each thread is listed once, even without a contact");
    assert_eq!(recent[0].peer, "friend@example.com");
    assert_eq!(recent[0].preview, "latest", "The last real message is the preview");
    assert!(recent[0].from_me);
    assert_eq!(recent[1].peer, "other@example.com");

    let _ = fs::remove_file(path);
}

#[test]
fn legacy_system_notices_are_purged_when_history_reopens() {
    let path = std::env::temp_dir().join(format!(
        "escargot-purge-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let history = History::open_at(&path, "me@example.com").unwrap();
        history.append("friend@example.com", &message("kept", false)).unwrap();
        history
            .append(
                "friend@example.com",
                &system_message("The conversation with Friend ended."),
            )
            .unwrap();
    }
    // Simulate rows written by builds that persisted system notices.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO messages (account, peer, from_me, author, text, system, at)
             VALUES ('me@example.com', 'friend@example.com', 0, '', 'Contact removed.', 1, '20:02')",
            [],
        )
        .unwrap();
    }

    let history = History::open_at(&path, "me@example.com").unwrap();
    let loaded = history.load("friend@example.com", 100).unwrap();
    assert_eq!(
        loaded.len(),
        1,
        "Reopening history must drop legacy system rows instead of replaying them"
    );
    assert_eq!(loaded[0].text, "kept");
    assert!(!loaded[0].system);

    let recent = history.recent_peers(10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].preview, "kept");

    let _ = fs::remove_file(path);
}
