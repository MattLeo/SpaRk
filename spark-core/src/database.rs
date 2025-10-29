use crate::{
    AuthError, error::Result, messages::{Message, MessageType, ReactionSummary, Room}, users::{Presence, Session, User}
};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use uuid::Uuid;
use regex::Regex;

pub struct Database {
    conn: Connection,
}

fn parse_presence(s: &str) -> Presence {
        match s {
            "Online" => Presence::Online,
            "Offline" => Presence::Offline,
            "Away" => Presence::Away,
            "DND" => Presence::DoNotDisturb,
            "AppearOffline" => Presence::AppearOffline,
            _ => Presence::Offline,
        }
    }


impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Database { conn };
        db.init()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Database { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_login TEXT,
                presence TEXT NOT NULL DEFAULT 'Offline',
                status TEXT
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id TEXT NOT NULL,
                token TEXT UNIQUE NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
            )", 
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS rooms (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                desc TEXT,
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE CASCADE
            )", [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS room_members (
                room_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                joined_at TEXT NOT NULL,
                PRIMARY KEY (room_id, user_id),
                FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )", [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                sender_id TEXT NOT NULL,
                message_type TEXT NOT NULL,
                room_id TEXT,
                receiver_id TEXT,
                content TEXT NOT NULL,
                sent_at TEXT NOT NULL,
                read_at TEXT,
                is_read INTEGER NOT NULL DEFAULT 0,
                is_edited INTEGER NOT NULL DEFAULT 0,
                edited_at TEXT,
                reply_to_message_id TEXT,
                reactions TEXT DEFAULT '[]',
                is_pinned INTEGER NOT NULL DEFAULT 0,
                pinned_at TEXT,
                pinned_by TEXT,
                FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (receiver_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE,
                FOREIGN KEY (reply_to_message_id) REFERENCES messages(id) ON DELETE SET NULL,
                FOREIGN KEY (pinned_by) REFERENCES users(id) ON DELETE SET NULL,
                CHECK (
                    (message_type = 'room' AND room_id IS NOT NULL AND receiver_id IS NULL) OR
                    (message_type = 'private' AND receiver_id IS NOT NULL AND room_id IS NULL) OR
                    (message_type = 'server' AND room_id IS NOT NULL AND receiver_id IS NULL)
                )
            )", [],
        )?;

        self.conn.execute("
            CREATE TABLE IF NOT EXISTS message_mentions (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                mentioned_user_id TEXT NOT NULL,
                is_read INTEGER NOT NULL DEFAULT 0,
                notified_at TEXT,
                read_at TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
                FOREIGN KEY (mentioned_user_id) REFERENCES users(id) ON DELETE CASCADE
            )",[]
        )?;

        // Indices

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_receiver ON messages(receiver_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_room ON messages(room_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_sent ON messages(sent_at)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_room_members_user ON room_members(user_id)",
            [],
        )?;
        
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mentions_user ON message_mentions(mentioned_user_id, is_read)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mentions_message ON message_mentions(message_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_reply_to ON messages(reply_to_message_id)",
            [],
        )?;  

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_pinned ON messages(room_id, is_pinned, pinned_at)",
            [],
        )?;

        Ok(())
    }

    // User Methods

    pub fn create_user(
        &self,
        username: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<User> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();

        self.conn.execute(
            "INSERT INTO users (id, username, email, password_hash, created_at, presence) VALUES (?1, ?2, ?3, ?4, ?5, 'Offline')",
            params![id, username, email, password_hash, now.to_rfc3339()],
        )?;

        Ok(User {
            id,
            username: username.to_string(),
            email: email.to_string(),
            password_hash: password_hash.to_string(),
            created_at: now,
            last_login: None,
            presence: Presence::Offline,
            status: None,
        })
    }


    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, email, password_hash, created_at, last_login, presence, status
            FROM users WHERE username = ?1"
        )?;

        let user = stmt.query_row(params![username], |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                email: row.get(2)?,
                password_hash: row.get(3)?,
                created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
                last_login: row.get::<_, Option<String>>(5)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                presence: parse_presence(&row.get::<_, String>(6)?),
                status: row.get(7)?,
            })
        });

        match user {
            Ok(u) => Ok(Some(u)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_user_by_id(&self, user_id: String) -> Result<Option<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, email, password_hash, created_at, last_login, presence, status
            FROM users WHERE id = ?1"
        )?;

        let user = stmt.query_row(params![user_id], |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                email: row.get(2)?,
                password_hash: row.get(3)?,
                created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
                last_login: row.get::<_, Option<String>>(5)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                presence: parse_presence(&row.get::<_, String>(6)?),
                status: row.get(7)?,
            })
        });

        match user {
            Ok(u) => Ok(Some(u)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update_last_login(&self, user_id: String) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "UPDATE users SET last_login = ?1 WHERE id = ?2",
            params!{now.to_rfc3339(), user_id},
        )?;
        Ok(())
    }

    pub fn update_user_presence(&self, user_id: &str, presence: &Presence) -> Result<()> {
        let presence_str = match presence {
            Presence::Online => "Online",
            Presence::Offline => "Offline",
            Presence::Away => "Away",
            Presence::DoNotDisturb => "DND",
            Presence::AppearOffline => "AppearOffline",
        };

        self.conn.execute("UPDATE users SET presence = ?1 WHERE id = ?2", params![presence_str, user_id])?;

        Ok(())
    }

    pub fn update_user_status(&self, user_id: &str, status: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE users SET status = ?1 WHERE id = ?2", params![status, user_id])?;
        Ok(())
    }

    // Session Methods

    pub fn create_session(&self, user_id: String, token: &str, expires_at: DateTime<Utc>) -> Result<Session> {
        let now = Utc::now();
        
        self.conn.execute(
            "INSERT INTO sessions (user_id, token, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            params![user_id, token, now.to_rfc3339(), expires_at.to_rfc3339()],
        )?;

        let id = self.conn.last_insert_rowid();

        Ok(Session {
            id,
            user_id,
            token: token.to_string(),
            created_at: now,
            expires_at,
        })
    }

    pub fn get_session_by_token(&self, token: &str) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, user_id, token, created_at, expires_at
            FROM sessions WHERE token = ?1"
        )?;

        let session = stmt.query_row(params![token], |row| {
            Ok(Session {
                id: row.get(0)?,
                user_id: row.get(1)?,
                token: row.get(2)?,
                created_at: row.get::<_, String>(3)?.parse::<DateTime<Utc>>().unwrap(),
                expires_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
            })
        });

        match session {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_session(&self, token: &str) -> Result<()> {
        self.conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }

    pub fn delete_expired_sessions(&self) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "DELETE FROM sessions WHERE expires_at < ?1",
            params![now.to_rfc3339()],
        )?;
        Ok(())
    }

    // Room Methods

    pub fn create_room(&self, name: &str, desc: &str, created_by: &str) -> Result<Room> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        self.conn.execute(
            "INSERT INTO rooms (id, name, desc, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, desc, created_by, now.to_rfc3339()],
        )?;

        self.add_user_to_room(&id, created_by)?;

        Ok(Room {
            id,
            name: name.to_string(),
            desc: desc.to_string(),
            created_by: created_by.to_string(),
            created_at: now,
        })
    }

    pub fn get_all_rooms(&self) -> Result<Vec<Room>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, desc, created_by, created_at FROM rooms ORDER BY created_at DESC"
        )?;

        let rooms = stmt.query_map([], |row| {
            Ok(Room {
                id: row.get(0)?,
                name: row.get(1)?,
                desc: row.get(2)?,
                created_by: row.get(3)?,
                created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
            })
        })?;

        let mut result = Vec::new();
        for room in rooms {
            result.push(room?);
        }

        Ok(result)
    }

    pub fn get_room_by_id(&self, room_id: &str) -> Result<Option<Room>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, desc, created_by, created_at FROM rooms where id = ?1"
        )?;

        let room = stmt.query_row(params![room_id], |row| {
            Ok(Room {
                id: row.get(0)?,
                name: row.get(1)?,
                desc: row.get(2)?,
                created_by: row.get(3)?,
                created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap()
            })
        });

        match room {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into())
        }
    }

    pub fn add_user_to_room(&self, room_id: &str, user_id: &str) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT OR IGNORE INTO room_members (room_id, user_id, joined_at) VALUES (?1, ?2, ?3)",
            params![room_id, user_id, now.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn remove_user_from_room(&self, room_id: &str, user_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM room_members WHERE room_id = ?1 AND user_id = ?2",
            params![room_id, user_id],
        )?;
        Ok(())
    }

    pub fn is_user_in_room(&self, room_id: &str, user_id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM room_members WHERE room_id = ?1 AND user_id = ?2",
            params![room_id, user_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn get_user_rooms(&self, user_id: &str) -> Result<Vec<Room>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.name, r.desc, r.created_by, r.created_at
            FROM rooms r
            JOIN room_members rm ON r.id = rm.room_id
            WHERE rm.user_id = ?1
            ORDER BY rm.joined_at DESC",
        )?;

        let rooms = stmt.query_map(params![user_id], |row| {
            Ok(Room {
                id: row.get(0)?,
                name: row.get(1)?,
                desc: row.get(2)?,
                created_by: row.get(3)?,
                created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
            })
        })?;

        let mut result = Vec::new();
        for room in rooms { result.push(room?); }
        Ok(result)
    }

    pub fn create_room_message(
        &self, 
        sender_id: &str, 
        room_id: &str, 
        content: &str,
        reply_to_message_id: Option<&str>
    ) -> Result<Message> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        self.conn.execute(
            "INSERT INTO messages (id, sender_id, message_type, room_id, content, 
                sent_at, is_read, is_edited, reply_to_message_id, reactions, is_pinned)
            VALUES (?1, ?2, 'room', ?3, ?4, ?5, 0, 0, ?6, '[]', 0)",
            params![id, sender_id, room_id, content, now.to_rfc3339(), reply_to_message_id],
        )?;

        Ok(Message {
            id,
            sender_id: sender_id.to_string(),
            message_type: MessageType::Room,
            room_id: Some(room_id.to_string()),
            receiver_id: None,
            content: content.to_string(),
            sent_at: now,
            read_at: None,
            is_read: false,
            is_edited: false,
            edited_at: None,
            reply_to_message_id: reply_to_message_id.map(|s| s.to_string()),
            reactions: Vec::new(),
            is_pinned: false,
            pinned_at: None,
            pinned_by: None,
        })
    }

    pub fn get_room_messages(&self, room_id: &str, limit: usize, offset: usize) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, message_type, room_id, content, sent_at, is_edited, edited_at, 
                reply_to_message_id, reactions, is_pinned, pinned_at, pinned_by
            FROM messages
            WHERE (message_type = 'room' OR message_type = 'server')  AND room_id = ?1
            ORDER BY sent_at DESC
            LIMIT ?2 OFFSET ?3"
        )?;

        let messages = stmt.query_map(params![room_id, limit, offset], |row| {
            let reactions_json: String = row.get(9)?;
            let reactions: Vec<ReactionSummary> = serde_json::from_str(&reactions_json).unwrap_or_default();

            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: match row.get::<_, String>(2)?.as_str() { 
                    "room" => MessageType::Room, 
                    "server" => MessageType::Server, 
                    _ => MessageType::Room },
                room_id: Some(row.get(3)?),
                receiver_id: None,
                content: row.get(4)?,
                sent_at: row.get::<_, String>(5)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: None,
                is_read: false,
                is_edited: row.get(6)?,
                edited_at: row.get::<_, Option<String>>(7)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                reply_to_message_id: row.get(8)?,
                reactions,
                is_pinned: row.get::<_, i32>(10)? != 0,
                pinned_at: row.get::<_, Option<String>>(11)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                pinned_by: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for message in messages { result.push(message?); }
        Ok(result)
    }

    // Private Message Methods

    pub fn create_private_message(&self, sender_id: &str, receiver_id: &str, content: &str) ->  Result<Message> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        
        self.conn.execute(
            "INSERT INTO messages (id, sender_id, message_type, receiver_id, content, sent_at, is_read, is_edited, reactions, is_pinned)
            VALUES (?1, ?2, 'private', ?3, ?4, ?5, 0, 0, '[]', 0)",
            params![id, sender_id, receiver_id, content, now.to_rfc3339()],
        )?;

        Ok(Message {
            id,
            sender_id: sender_id.to_string(),
            message_type: MessageType::Private,
            room_id: None,
            receiver_id: Some(receiver_id.to_string()),
            content: content.to_string(),
            sent_at: now,
            read_at: None,
            is_read: false,
            is_edited: false,
            edited_at: None,
            reply_to_message_id: None,
            reactions: Vec::new(),
            is_pinned: false,
            pinned_at: None,
            pinned_by: None,
        })
    }

    pub fn get_private_messages_between_users(&self, user1_id: &str, user2_id: &str, limit: usize, offset: usize) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, receiver_id, content, sent_at, read_at, is_read, 
                is_edited, edited_at, reactions, is_pinned, pinned_at, pinned_by
            FROM messages
            WHERE message_type = 'private'
                AND ((sender_id = ?1 AND receiver_id = ?2) OR (sender_id = ?2 AND receiver_id = ?1))
            ORDER BY sent_at DESC
            LIMIT ?3 OFFSET ?4"
        )?;

        let messages = stmt.query_map(params![user1_id, user2_id, limit, offset], |row| {
            let reactions_json: String = row.get(9)?;
            let reactions: Vec<ReactionSummary> = serde_json::from_str(&reactions_json).unwrap_or_default();

            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: MessageType::Private,
                room_id: None,
                receiver_id: Some(row.get(2)?),
                content: row.get(3)?,
                sent_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: row.get::<_, Option<String>>(5)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                is_read: row.get::<_, i32>(6)? != 0,
                is_edited: row.get(7)?,
                edited_at: row.get::<_, Option<String>>(8)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                reply_to_message_id: None,
                reactions,
                is_pinned: row.get::<_, i32>(10)? != 0,
                pinned_at: row.get::<_, Option<String>>(11)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                pinned_by: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }
        Ok(result)
    }

    pub fn get_received_private_messages(&self, receiver_id: &str, unread_only: bool, limit: usize, offset: usize) -> Result<Vec<Message>> {
        let query = if unread_only {
            "SELECT id, sender_id, receiver_id, content, sent_at, read_at, is_read, is_edited, edited_at, reactions, is_pinned, pinned_at, pinned_by
            FROM messages
            WHERE message_type = 'private' AND receiver_id = ?1 AND is_read = 0
            ORDER BY sent_at DESC
            LIMIT ?2 OFFSET ?3"
        } else {
            "SELECT id, sender_id, receiver_id, content, sent_at, read_at, is_read, is_edited, edited_at, reactions, is_pinned, pinned_at, pinned_by
            FROM messages
            WHERE message_type = 'private' AND receiver_id = ?1
            ORDER BY sent_at DESC
            LIMIT ?2 OFFSET ?3"
        };

        let mut stmt = self.conn.prepare(query)?;

        let messages = stmt.query_map(params![receiver_id, limit, offset], |row| {
            let reactions_string: String = row.get(9)?;
            let reactions = serde_json::from_str(&reactions_string).unwrap_or_default();

            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: MessageType::Private,
                room_id: None,
                receiver_id: Some(row.get(2)?),
                content: row.get(3)?,
                sent_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: row.get::<_, Option<String>>(5)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                is_read: row.get::<_, i32>(6)? != 0,
                is_edited: row.get(7)?,
                edited_at: row.get::<_, Option<String>>(8)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                reply_to_message_id: None,
                reactions,
                is_pinned: row.get::<_, i32>(10)? != 0,
                pinned_at: row.get::<_, Option<String>>(11)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                pinned_by: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }
        Ok(result)
    }

    pub fn mark_private_message_as_read(&self, message_id: &str) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "UPDATE messages SET is_read = 1, read_at = ?1
            WHERE id = ?2 AND message_type = 'private'", 
            params![now.to_rfc3339(), message_id]
        )?;
        Ok(())
    }

    pub fn mark_private_conversation_as_read(&self, receiver_id: &str, sender_id: &str) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "UPDATE messages SET is_read = 1, read_at = ?1
            WHERE message_type = 'private' AND receiver_id = ?2 AND sender_id = ?3 AND is_read = 0", 
            params![now.to_rfc3339(), receiver_id, sender_id]
        )?;
        Ok(())
    }

    pub fn get_unread_private_message_count(&self, user_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages
            WHERE message_type = 'private' AND receiver_id = ?1 AND is_read = 0",
            params![user_id],
            |row| row.get(0)
        )?;

        Ok(count)
    }

    pub fn delete_message(&self, message_id: &str, user1_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM MESSAGES WHERE id = ?1 AND sender_id = ?2", 
            params![message_id, user1_id]
        )?;
        Ok(())
    }
    pub fn edit_message(&self, message_id: &str, content: &str) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "UPDATE messages SET content = ?1, is_edited = 1, edited_at = ?2 WHERE id = ?3", 
            params![content, now.to_rfc3339(), message_id]
        )?;
        Ok(())
    }

    pub fn room_announcement(&self, room_id: &str, content: &str, sender_id: &str) -> Result<Message> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();

        self.conn.execute(
            "INSERT INTO messages (id, sender_id, message_type, room_id, content, sent_at, is_read, is_edited, reactions, is_pinned)
            VALUES (?1, ?2, 'server', ?3, ?4, ?5, 0, 0, '[]', 0)",
            params![id, sender_id, room_id, content, now.to_rfc3339()],
        )?;

        Ok(Message {
            id,
            sender_id: sender_id.to_string(),
            message_type: MessageType::Server,
            room_id: Some(room_id.to_string()),
            receiver_id: None,
            content: content.to_string(),
            sent_at: now,
            read_at: None,
            is_read: false,
            is_edited: false,
            edited_at: None,
            reply_to_message_id: None,
            reactions: Vec::new(),
            is_pinned: false,
            pinned_at: None,
            pinned_by: None,
        })
    }

    pub fn get_room_members(&self, room_id: &str) -> Result<Vec<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT u.id, u.username, u.email, u.password_hash, u.created_at, u.last_login, u.presence, u.status
            FROM users u
            JOIN room_members rm ON u.id = rm.user_id
            WHERE rm.room_id = ?1
            ORDER BY u.username" // This will need to be changed to role once user roles are added
        )?;

        let members = stmt.query_map(params![room_id], |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                email: row.get(2)?,
                password_hash: row.get(3)?,
                created_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
                last_login: row.get::<_, Option<String>>(5)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                presence: parse_presence(&row.get::<_, String>(6)?),
                status: row.get(7)?,
            })
        })?;

        let mut result = Vec::new();
        for member in members {
            result.push(member?);
        }

        Ok(result)
    }

    pub fn extract_mentions(&self, content: &str) -> Vec<String> {
        let re = Regex::new(r"@(\w+)").unwrap();
        re.captures_iter(content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .filter(|username| username.to_lowercase() != "everyone")
            .collect()
    }

    pub fn everyone_mentioned(&self, content: &str) -> bool {
        content.to_lowercase().contains("@everyone")
    }

    pub fn save_message_mentions(&self, message_id: &str, sender_id: &str, content: &str, room_id: &str) -> Result<Vec<String>> {
        let mut notified_user_ids = Vec::new();
        let now = Utc::now();

        if self.everyone_mentioned(content) {
            let members = self.get_room_members(room_id)?;

            for member in members {
                if member.id != sender_id {
                    let mention_id = Uuid::new_v4().to_string();
                    self.conn.execute(
                        "INSERT INTO message_mentions (id, message_id, mentioned_user_id, notified_at, created_at)
                        VALUES (?1, ?2, ?3, ?4, ?5)", 
                        params![mention_id, message_id, member.id, now.to_rfc3339(), now.to_rfc3339()],
                    )?;
                    notified_user_ids.push(member.id);
                }
            }
        } else {
            let mentioned_usernames = self.extract_mentions(content);

            for username in mentioned_usernames {
                if let Ok(Some(user)) = self.get_user_by_username(&username) {
                    if user.id != sender_id {
                        let mention_id = Uuid::new_v4().to_string();
                        self.conn.execute(
                            "INSERT INTO message_mentions (id, message_id, mentioned_user_id, notified_at, created_at)
                            VALUES (?1, ?2, ?3, ?4, ?5)", 
                            params![mention_id, message_id, user.id, now.to_rfc3339(), now.to_rfc3339()],
                        )?;
                        notified_user_ids.push(user.id);
                    }
                }
            }
        }
        Ok(notified_user_ids)
    }

    pub fn get_message_mentions(&self, message_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT mentioned_user_ids FROM message_mentions WHERE message_id = ?1",
        )?;

        let user_ids = stmt.query_map(params![message_id], |row| {
            row.get::<_, String>(0)
        })?;

        let mut result = Vec::new();
        for user_id in user_ids {
            result.push(user_id?);
        }
        Ok(result)
    }

    pub fn get_unread_mentions_count(&self, user_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM message_mentions WHERE mentioned_user_id = ?1 AND is_read = 0",
            params![user_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn mark_mention_as_read(&self, user_id: &str, message_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE message_mentions SET is_read = 1, read_at = ?1
            WHERE mentioned_user_id = ?2 AND message_id = ?3", 
            params![now, user_id, message_id],
        )?;
        Ok(())
    }

    pub fn mark_room_mentions_as_read(&self, user_id: &str, room_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE message_mentions SET is_read = 1, read_at = ?1
            WHERE mentioned_user_id = ?2
            AND message_id IN (
                SELECT id FROM messages WHERE room_id = ?3
            )
            AND is_read = 0", 
            params![now, user_id, room_id],
        )?;

        Ok(())
    }

    pub fn get_all_user_mentions(&self, user_id: &str, limit: usize, offset: usize) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.sender_id, m.message_type, m.room_id, m.content, m.sent_at, 
                m.is_edited, m.edited_at, m.reply_to_message_id, m.reactions, m.is_pinned, m.pinned_at, m.pinned_by
            FROM messages m
            JOIN message_mentions mm ON m.id = mm.message_id
            WHERE mm.mentioned_user_id = ?1
            ORDER BY m.sent_at DESC
            LIMIT ?2 OFFSET ?3"
        )?;

        let messages = stmt.query_map(params![user_id, limit, offset], |row| {
            let reactions_string: String = row.get(9)?;
            let reactions = serde_json::from_str(&reactions_string).unwrap_or_default();

            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: match row.get::<_, String>(2)?.as_str() {
                    "server" => MessageType::Server,
                    "private" => MessageType::Private,
                    _ => MessageType::Room,
                },
                room_id: Some(row.get(3)?),
                receiver_id: None,
                content: row.get(4)?,
                sent_at: row.get::<_, String>(5)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: None,
                is_read: false,
                is_edited: row.get(6)?,
                edited_at: row.get::<_, Option<String>>(7)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                reply_to_message_id: row.get(8)?,
                reactions,
                is_pinned: row.get::<_, i32>(10)? != 0,
                pinned_at: row.get::<_, Option<String>>(11)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                pinned_by: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }
        Ok(result)
    }

    pub fn get_message_by_id(&self, message_id: &str) -> Result<Option<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, message_type, room_id, receiver_id, content, sent_at,
                read_at, is_read, is_edited, edited_at, reply_to_message_id, reactions, is_pinned, pinned_at, pinned_by
            FROM messages
            WHERE id = ?1"
        )?;

        let message = stmt.query_row(params![message_id], |row| {
            let reactions_string: String = row.get(12)?;
            let reactions = serde_json::from_str(&reactions_string).unwrap_or_default();

            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: match row.get::<_, String>(2)?.as_str() {
                    "private" => MessageType::Private,
                    "server" => MessageType::Server,
                    _ => MessageType::Room,
                },
                room_id: row.get(3)?,
                receiver_id: row.get(4)?,
                content: row.get(5)?,
                sent_at: row.get::<_, String>(6)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: row.get::<_, Option<String>>(7)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                is_read: row.get(8)?,
                is_edited: row.get(9)?,
                edited_at: row.get::<_, Option<String>>(10)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                reply_to_message_id: row.get(11)?,
                reactions,
                is_pinned: row.get::<_,i32>(13)? != 0,
                pinned_at: row.get::<_, Option<String>>(14)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                pinned_by: row.get(15)?,
            })
        });

        match message {
            Ok(msg) => Ok(Some(msg)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn add_reaction(&self, message_id: &str, user_id: &str, username: &str, emoji: &str) -> Result<Vec<ReactionSummary>> {
        let current_reactions: String = self.conn.query_row(
            "SELECT COALESCE(reactions, '[]') FROM messages WHERE id = ?1", 
            params![message_id],
            |row| row.get(0),
        )?;

        let mut reactions: Vec<ReactionSummary> = serde_json::from_str(&current_reactions)
            .map_err(|e| AuthError::InvalidInput(format!("Failed to parse reactions: {}", e)))?;

        if let Some(reaction) = reactions.iter_mut().find(|r| r.emoji == emoji) {
            if !reaction.user_ids.contains(&user_id.to_string()) {
                reaction.user_ids.push(user_id.to_string());
                reaction.usernames.push(username.to_string());
                reaction.count += 1;
            } 
        } else {
            reactions.push(ReactionSummary { 
                emoji: emoji.to_string(), 
                count: 1, 
                user_ids: vec![user_id.to_string()], 
                usernames: vec![username.to_string()], 
            });
        }

        let reactions_json = serde_json::to_string(&reactions)
            .map_err(|e| AuthError::InvalidInput(format!("Failed to serialize reactions: {}", e)))?;
        self.conn.execute(
            "UPDATE messages SET reactions = ?1 WHERE id = ?2",
            params![reactions_json, message_id]
        )?;

        Ok(reactions)
    }

    pub fn remove_reaction(&self, message_id: &str, user_id: &str, emoji: &str) -> Result<Vec<ReactionSummary>> {
        let current_reactions: String = self.conn.query_row(
            "SELET COALESCE(reactions, '[]') FROM messages WHERE id = ?1", 
            params![message_id], 
            |row| row.get(0),
        )?;

        let mut reactions: Vec<ReactionSummary> = serde_json::from_str(&current_reactions)
            .map_err(|e| AuthError::InvalidInput(format!("Failed to parse reactions: {}", e)))?;

        for reaction in reactions.iter_mut() {
            if reaction.emoji == emoji {
                if let Some(pos) = reaction.user_ids.iter().position(|id| id == user_id) {
                    reaction.user_ids.remove(pos);
                    reaction.usernames.remove(pos);
                    reaction.count = reaction.count.saturating_sub(1);
                }
            }
        } 

        reactions.retain(|r| r.count > 0);

        let reactions_json = serde_json::to_string(&reactions)
            .map_err(|e| AuthError::InvalidInput(format!("Failed to serialize reactions: {}", e)))?;
        self.conn.execute(
            "UPDATE messages SET reactions = ?1 WHERE id = ?2", 
            params![reactions_json, message_id],
        )?;

        Ok(reactions)
    }

    pub fn pin_message(&self, message_id: &str, user_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE messages SET is_pinned = 1, pinned_at = ?1, pinned_by = ?2 WHERE id = ?3", 
            params![now, user_id, message_id],
        )?;
        Ok(())
    }

    pub fn unpin_message(&self, message_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET is_pinned = 0, pinned_at = NULL, pinned_by = NULL WHERE id = ?1", 
            [message_id],
        )?;
        Ok(())
    }

    pub fn get_pinned_messages(&self, room_id: &str) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender_id, message_type, room_id, content, sent_at, is_edited, edited_at,
                reply_to_message_id, reactions, is_pinned, pinned_at, pinned_by
            FROM messages
            WHERE room_id = ?1 AND is_pinned = 1
            ORDER BY pinned_at DESC"
        )?;

        let messages = stmt.query_map(params![room_id], |row| {
            let reactions_string: String = row.get(9)?;
            let reactions: Vec<ReactionSummary> = serde_json::from_str(&reactions_string).unwrap_or_default();

            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: match row.get::<_, String>(2)?.as_str() {
                    "server" => MessageType::Server,
                    "private" => MessageType::Private,
                    _ => MessageType::Room,
                },
                room_id: Some(row.get(3)?),
                receiver_id: None,
                content: row.get(4)?,
                sent_at: row.get::<_, String>(5)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: None,
                is_read: false,
                is_edited: row.get(6)?,
                edited_at: row.get::<_, Option<String>>(7)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                reply_to_message_id: row.get(8)?,
                reactions,
                is_pinned: row.get::<_, i32>(10)? != 0,
                pinned_at: row.get::<_, Option<String>>(11)?.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                pinned_by: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }
        Ok(result)
    }

}