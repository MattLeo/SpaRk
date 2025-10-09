use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Database Error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Password hashing error: {0}")]
    PasswordHash(String),

    #[error("InvalidC Credentials")]
    InvalidCredentials,

    #[error("User already exists")]
    UserExists,

    #[error("User not found")]
    UserNotFound,

    #[error("Session not found or expired")]
    InvalidSession,

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;