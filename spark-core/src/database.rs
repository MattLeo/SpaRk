use crate::{
    error::Result, messages::{self, Message, MessageType, Room, RoomMember}, users::{Session, User}
};
use chrono::{Date, DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use uuid::Uuid;

pub struct Database {
    conn: Connection,
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
                last_login TEXT
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
                FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (receiver_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (room_id) REFERENCS rooms(id) ON DELETE CASCADE,
                CHECK (
                    (message_type = 'room' AND room_id IS NOT NULL AND receiver_id IS NULL) OR
                    (message_type = 'private' AND receiver_id IS NOT NULL AND room_id IS NULL)
                )
            )", [],
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
            "INSERT INTO users (id, username, email, password_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, username, email, password_hash, now.to_rfc3339()],
        )?;

        Ok(User {
            id,
            username: username.to_string(),
            email: email.to_string(),
            password_hash: password_hash.to_string(),
            created_at: now,
            last_login: None,
        })
    }


    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, email, password_hash, created_at, last_login 
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
            "SELECT id, username, email, password_hash, created_at, last_login
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
            params![id, name, created_by, now.to_rfc3339()],
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
            "INSERT OR IGNORE INTO room_members (rood_id, user_id, joined_at) VALUES (?1, ?2. ?3)",
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
            "SELECT r.id, r.name, r.created_by, r.created_at
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

    pub fn create_room_message(&self, sender_id: &str, room_id: &str, content: &str) -> Result<Message> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        self.conn.execute(
            "INSERT INTO messages (id, sender_id, message_id, room_id, content, sent_at, is_read)
            VALUES (?1, ?2, 'room', ?3, ?4, ?5, 0)",
            params![id, sender_id, room_id, content, now.to_rfc3339()],
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
        })
    }

    pub fn get_room_messages(&self, room_id: &str, limit: usize, offset: usize) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELET id, sender_id, room_id, content, sent_at
            FROM messages
            WHERE message_type = 'room' AND room_id = ?1
            ORDER BY sent_at DESC
            LIMIT ?2 OFFSET ?3"
        )?;

        let messages = stmt.query_map(params![room_id, limit, offset], |row| {
            Ok(Message {
                id: row.get(0)?,
                sender_id: row.get(1)?,
                message_type: MessageType::Room,
                room_id: Some(row.get(2)?),
                receiver_id: None,
                content: row.get(3)?,
                sent_at: row.get::<_, String>(4)?.parse::<DateTime<Utc>>().unwrap(),
                read_at: None,
                is_read: false,
            })
        })?;

        let mut result = Vec::new();
        for message in messages { result.push(message?); }
        Ok(result)
    }

    // Private Message Methods



}