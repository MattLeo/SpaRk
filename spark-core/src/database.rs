use crate::{error::Result, users::{User, Session}};
use chrono::{DateTime, Utc};
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
            "CREATE TABLE IF NOT EXISTS uesrs (
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
                FOREIGN KEY(user_id) REFERENCS users(id) ON DELETE CASCADE
            )", 
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id)",
            [],
        )?;

        Ok(())
    }

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
            "SELECT id, username, email, password_has, created_at, last_login
            FROM uesrs WHERE id = ?1"
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

    pub fn create_session(&self, user_id: String, token: &str, expires_at: DateTime<Utc>) -> Result<Session> {
        let now = Utc::now();
        
        self.conn.execute(
            "INSERT INTO sessions (user_id, token, created_at, exprires_at) VALUES (?1, ?2, ?3, ?4)",
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

}