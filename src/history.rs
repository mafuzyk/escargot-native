//! Local conversation history. MSN never stored 7.5 conversations on the
//! server; the client rendered them from its own local log. This module keeps
//! that behaviour in SQLite under `~/.local/share/escargot-native`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::state::ChatMessage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentPeer {
    pub peer: String,
    pub preview: String,
    pub at: String,
    pub from_me: bool,
}

pub struct History {
    conn: Connection,
    account: String,
}

impl History {
    pub fn open(account: &str) -> rusqlite::Result<Self> {
        Self::open_at(&history_path(), account)
    }

    pub fn open_at(path: &Path, account: &str) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account TEXT NOT NULL,
                peer TEXT NOT NULL,
                from_me INTEGER NOT NULL,
                author TEXT NOT NULL,
                text TEXT NOT NULL,
                system INTEGER NOT NULL,
                at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS messages_lookup
                ON messages(account, peer, id);",
        )?;
        // Older builds persisted transient system notices. Drop them once so a
        // conversation does not replay "conversation ended" lines after every
        // reconnect or upgrade.
        conn.execute("DELETE FROM messages WHERE system != 0", [])?;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        Ok(Self { conn, account: account.to_ascii_lowercase() })
    }

    pub fn load(&self, peer: &str, limit: usize) -> rusqlite::Result<Vec<ChatMessage>> {
        let mut statement = self.conn.prepare(
            "SELECT from_me, author, text, system, at
             FROM messages
             WHERE account = ?1 AND peer = ?2
             ORDER BY id DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![self.account, peer.to_ascii_lowercase(), limit as i64],
            |row| {
                Ok(ChatMessage {
                    id: 0,
                    from_me: row.get::<_, i64>(0)? != 0,
                    author: row.get(1)?,
                    text: row.get(2)?,
                    system: row.get::<_, i64>(3)? != 0,
                    at: row.get(4)?,
                    delivery: crate::state::Delivery::Sent,
                })
            },
        )?;

        let mut messages = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok(messages)
    }

    /// Threads ordered by their last real message, independent of the contact
    /// list. This keeps a conversation reachable after its contact is removed.
    pub fn recent_peers(&self, limit: usize) -> rusqlite::Result<Vec<RecentPeer>> {
        let mut statement = self.conn.prepare(
            "SELECT m.peer, m.text, m.at, m.from_me
             FROM messages m
             JOIN (
                 SELECT peer, MAX(id) AS last_id
                 FROM messages
                 WHERE account = ?1 AND system = 0
                 GROUP BY peer
             ) latest ON latest.last_id = m.id
             ORDER BY m.id DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![self.account, limit as i64], |row| {
            Ok(RecentPeer {
                peer: row.get(0)?,
                preview: row.get(1)?,
                at: row.get(2)?,
                from_me: row.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn append(&self, peer: &str, message: &ChatMessage) -> rusqlite::Result<()> {
        // System notices are ephemeral UI state, never conversation history.
        if message.system {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO messages (account, peer, from_me, author, text, system, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                self.account,
                peer.to_ascii_lowercase(),
                message.from_me as i64,
                message.author,
                message.text,
                message.system as i64,
                message.at,
            ],
        )?;
        Ok(())
    }
}

fn history_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("escargot-native")
        .join("history.db")
}
