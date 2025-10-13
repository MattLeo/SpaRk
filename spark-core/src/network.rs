use crate::{
    Database, error::{AuthError, Result}, messages::{GetPrivateMessagesRequest, GetRoomMessagesRequest, Message, PrivateMessageResponse, RoomMessageResponse, SendPrivateMessageRequest, SendRoomMessageRequest}, users::{AuthResponse, CreateUserRequest, LoginRequest, User}
};
use argon2::{
    password_hash::{self, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2
};
use chrono::{Duration, Utc};
use rand::{distributions::{Alphanumeric}, Rng};
use std::sync::Arc;


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

pub struct MessageService {
    db: Database
}

impl MessageService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    fn validate_message_content(&self, content: &str) -> Result<()> {
        if content.trim().is_empty() {
            return Err(AuthError::InvalidInput("Message content cannot be empty".to_string()));
        }

        if content.len() > 10000 {
            return Err(AuthError::InvalidInput("Message too long (max 10,000 characters".to_string()));
        }

        Ok(())
    }

    pub fn send_room_message(&self, sender_id: &str, request: SendRoomMessageRequest) -> Result<RoomMessageResponse> {
        self.validate_message_content(&request.content)?;

        if !self.db.is_user_in_room(&request.room_id, sender_id)? {
            return Err(AuthError::InvalidInput("You are not a member of this room".to_string()));
        }

        let room = self.db.get_room_by_id(&request.room_id)?.ok_or(AuthError::InvalidInput("Room not found".to_string()))?;
        let sender = self.db.get_user_by_id(sender_id.to_string())?.ok_or(AuthError::UserNotFound)?;
        let message = self.db.create_room_message(sender_id, &request.room_id, &request.content)?;

        Ok(RoomMessageResponse {
            id: message.id,
            sender_username: sender.username,
            room_id: room.id,
            room_name: room.name,
            content: message.content,
            sent_at: message.sent_at,
        })
    }

    pub fn get_room_messages(&self, user_id: &str, request: GetRoomMessagesRequest) -> Result<Vec<RoomMessageResponse>> {
        if !self.db.is_user_in_room(&request.room_id, user_id)? {
            return Err(AuthError::InvalidInput("You are not a member of this room".to_string()));
        }

        let limit = request.limit.unwrap_or(50).min(100);
        let offset = request.offset.unwrap_or(0);
        let messages = self.db.get_room_messages(&request.room_id, limit, offset)?;
        let room = self.db.get_room_by_id(&request.room_id)?.ok_or(AuthError::InvalidInput("Room not found".to_string()))?;

        let mut responses = Vec::new();
        for msg in messages {
            let sender = self.db.get_user_by_id(msg.sender_id.clone())?.ok_or(AuthError::UserNotFound)?;

            responses.push(RoomMessageResponse {
                id: msg.id,
                sender_username: sender.username,
                room_id: room.id.clone(),
                room_name: room.name.clone(),
                content: msg.content,
                sent_at: msg.sent_at
            })
        }
        Ok(responses)
    }

    pub fn send_private_message(&self, sender_id: &str, request: SendPrivateMessageRequest) -> Result<PrivateMessageResponse> {
        let receiver = self.db.get_user_by_username(&request.receiver_username)?.ok_or(AuthError::UserNotFound)?;
        let sender = self.db.get_user_by_id(sender_id.to_string())?.ok_or(AuthError::UserNotFound)?;
        let message = self.db.create_private_message(sender_id, &receiver.id, &request.content)?;

        Ok(PrivateMessageResponse { 
            id: message.id, 
            sender_username: sender.username, 
            receiver_username: receiver.username, 
            content: message.content, 
            sent_at: message.sent_at, 
            read_at: message.read_at, 
            is_read: message.is_read, 
        })
    }

    pub fn get_private_messages(&self, user_id: &str, request: GetPrivateMessagesRequest) -> Result<Vec<PrivateMessageResponse>> {
        let limit = request.limit.unwrap_or(50).min(100);
        let offset = request.limit.unwrap_or(0);

        let messages = if let Some(other_username) = request.with_user {
            let other_user = self.db.get_user_by_username(&other_username)?.ok_or(AuthError::UserNotFound)?;
            self.db.get_private_messages_between_users(user_id, &other_user.id, limit, offset)?
        } else {
            self.db.get_received_private_messages(user_id, request.unread_only, limit, offset)?
        };

        let mut responses = Vec::new();
        for msg in messages {
            let sender = self.db.get_user_by_id(msg.sender_id.clone())?.ok_or(AuthError::UserNotFound)?;
            let receiver = self.db.get_user_by_id(msg.receiver_id.clone().unwrap())?.ok_or(AuthError::UserNotFound)?;

            responses.push(PrivateMessageResponse {
                id: msg.id,
                sender_username: sender.username,
                receiver_username: receiver.username,
                content: msg.content,
                sent_at: msg.sent_at,
                read_at: msg.read_at,
                is_read: msg.is_read,
            });
        } 

        Ok(responses)
    }

    pub fn mark_private_messages_as_read(&self, message_id: &str) -> Result<()> {
        self.db.mark_private_message_as_read(message_id)?;
        Ok(())
    }

    pub fn mark_private_conversation_as_read(&self, user_id: &str, other_username: &str) -> Result<()> {
        let other_user = self.db.get_user_by_username(other_username)?.ok_or(AuthError::UserNotFound)?;

        self.db.mark_private_conversation_as_read(user_id, &other_user.id)?;
        Ok(())
    }

    pub fn get_unread_private_message_count(&self, user_id: &str) -> Result<i64> {
        self.db.get_unread_private_message_count(user_id)
    }

    pub fn delete_message(&self, user_id: &str, message_id: &str) -> Result<()> {
        self.db.delete_message(message_id, user_id)?;
        Ok(())
    }

    pub fn join_room(&self, user_id: &str, room_id: &str) -> Result<()> {
        self.db.add_user_to_room(room_id, user_id)?;
        Ok(())
    }

    pub fn leave_room(&self, user_id: &str, room_id: &str) -> Result<()> {
        self.db.remove_user_from_room(room_id, user_id)?;
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

    #[test]
    fn test_room_message_creation() {
        let db = Database::in_memory().unwrap();
        let user = db.create_user("Testuser", "test@test.com", "hash123").unwrap();

        let room = db.create_room("General", "general test room", &user.id).unwrap();
        let message = db.create_room_message(&user.id, &room.id, "Hello!").unwrap();

        assert_eq!(message.content, "Hello!");
        assert_eq!(message.room_id, Some(room.id));
    }

    #[test]
    fn test_send_room_message() {
        let db = Database::in_memory().unwrap();

        let user = db.create_user("testuser", "test@test.com", "hash123").unwrap();
        let room = db.create_room("General", "test room", &user.id).unwrap();
        let message = db.create_room_message(&user.id, &room.id, "Hello room!").unwrap();

        assert_eq!(message.content, "Hello room!");
        assert_eq!(message.room_id, Some(room.id.clone()));
        assert_eq!(message.sender_id, user.id);
        
        let messages = db.get_room_messages(&room.id, 10, 0).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello room!");
    }
}