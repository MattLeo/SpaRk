use crate::{
    database::Database,
    error::{AuthError, Result},
    users::{AuthResponse, CreateUserRequest, LoginRequest, User}
};
use argon2::{
    password_hash::{self, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2
};
use chrono::{Duration, Utc};
use rand::{distributions::{Alphanumeric}, Rng};


pub struct AuthService {
    db: Database,
}

impl AuthService {
    pub fn new(db: Database) -> Self {
        Self{ db }
    }

    fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut rand::thread_rng());
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AuthError::PasswordHash(e.to_string()))
    }

    fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AuthError::PasswordHash(e.to_string()))?;

        Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
    }

    fn generate_token(&self) -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect()
    }

    fn validate_credentials(&self, username: &str, email: &str, password: &str) -> Result<()> {
        if username.is_empty() || username.len() < 3 {
            return Err(AuthError::InvalidInput("Username must be at least 3 characters long".to_string()))
        };

        if username.len() > 50 {
            return Err(AuthError::InvalidInput("Username must be less than 50 characters long".to_string()))
        };

        if !email.contains('@') || email.len()< 5 {
            return Err(AuthError::InvalidInput("Invalid email format".to_string()));
        }

        if password.len() < 8 {
            return Err(AuthError::InvalidInput("Password must be at least 8 characters long".to_string()))
        }

        Ok(())
    }

    pub fn register(&self, request: CreateUserRequest) -> Result<AuthResponse> {
        self.validate_credentials(&request.username, &request.email, &request.password)?;

        if self.db.get_user_by_username(&request.username)?.is_some() {
            return Err(AuthError::UserExists);
        }

        let password_hash = self.hash_password(&request.password)?;
        let user = self.db.create_user(&request.username, &request.email, &password_hash)?;

        let token = self.generate_token();
        let expires_at = Utc::now() + Duration::days(30);
        self.db.create_session(user.id.clone(), &token, expires_at)?;

        self.db.update_last_login(user.id.clone())?;

        Ok(AuthResponse { user, token })
    }

    pub fn login(&self, request: LoginRequest) -> Result<AuthResponse> {
        let user = self.db
            .get_user_by_username(&request.username)?
            .ok_or(AuthError::InvalidCredentials)?;

        if !self.verify_password(&request.password, &user.password_hash)? {
            return Err(AuthError::InvalidCredentials);
        }

        let token = self.generate_token();
        let expires_at = Utc::now() + Duration::days(30);
        self.db.create_session(user.id.clone(), &token, expires_at)?;
        self.db.update_last_login(user.id.clone())?;

        Ok(AuthResponse { user, token })
    }

    pub fn validate_session(&self, token: &str) -> Result<User> {
        let session = self.db
            .get_session_by_token(token)?
            .ok_or(AuthError::InvalidSession)?;

        if session.expires_at < Utc::now() {
            self.db.delete_session(token)?;
            return Err(AuthError::InvalidSession);
        }

        let user = self.db
            .get_user_by_id(session.user_id)?
            .ok_or(AuthError::UserNotFound)?;

        Ok(user)
    }

    pub fn logout(&self, token: &str) -> Result<()> {
        self.db.delete_session(token)?;
        Ok(())
    }

    pub fn cleanup_expired_sessions(&self) -> Result<()> {
        self.db.delete_expired_sessions()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let db = Database::in_memory().unwrap();
        let auth = AuthService::new(db);

        let password = "test_password_123";
        let hash = auth.hash_password(password).unwrap();

        assert!(auth.verify_password(password, &hash).unwrap());
        assert!(!auth.verify_password("bad_password_321", &hash).unwrap());
    }

    #[test]
    fn test_register_and_login() {
        let db = Database::in_memory().unwrap();
        let auth = AuthService::new(db);

        let register_req = CreateUserRequest {
            username: "testuser".to_string(),
            email: "testemail@test.com".to_string(),
            password: "test_password_123".to_string(),
        };

        let register_response = auth.register(register_req).unwrap();
        assert_eq!(register_response.user.username, "testuser");
        assert!(!register_response.token.is_empty());

        let login_req = LoginRequest {
            username: "testuser".to_string(),
            password: "test_password_123".to_string(),
        };

        let login_response = auth.login(login_req).unwrap();
        assert_eq!(login_response.user.username, "testuser");
        assert!(!login_response.token.is_empty());
    }

    #[test]
    fn test_session_validation() {
        let db = Database::in_memory().unwrap();
        let auth = AuthService::new(db);

        let register_req = CreateUserRequest {
            username: "testuser".to_string(),
            email: "test_eamail@test.com".to_string(),
            password: "test_password_123".to_string(),
        };

        let response = auth.register(register_req).unwrap();
        let user = auth.validate_session(&response.token).unwrap();

        assert_eq!(user.username, "testuser");
        auth.logout(&response.token).unwrap();

        assert!(auth.validate_session(&response.token).is_err());
    }
}