use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// WebSocket messages for signaling and presence
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    // Client -> Server
    Join(JoinRequest),
    Leave,
    Offer {
        to: String,
        sdp: String,
    },
    Answer {
        to: String,
        sdp: String,
    },
    IceCandidate {
        to: String,
        candidate: String,
    },
    ChatMessage {
        message: String,
    },

    // Server -> Client
    JoinedAs {
        username: Username,
        token: String,
        is_owner: bool,
    },
    MemberJoined {
        username: Username,
    },
    MemberLeft {
        username: Username,
    },
    MemberList {
        members: Vec<MemberInfo>,
    },
    SignalingMessage {
        from: String,
        payload: SignalingPayload,
    },
    Chat {
        from: Username,
        message: String,
    },
    Error(ErrorKind),
    ForceDisconnect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JoinRequest {
    WithToken {
        token: String,
    },
    WithUsername {
        username: Username,
        password: Option<Password>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignalingPayload {
    Offer { sdp: String },
    Answer { sdp: String },
    IceCandidate { candidate: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ErrorKind {
    TokenNotFound,
    TokenAlreadyInUse,
    RoomNotFound,
    InvalidUsername { message: String },
    UsernameTaken,
    PasswordRequired,
    IncorrectPassword,
    JoinTimeout,
    TooManyAttempts,
    Other { message: String },
}

/// A validated room ID (UUID format)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RoomId(String);

impl RoomId {
    /// Get the room ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner String
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct RoomIdError(String);

impl fmt::Display for RoomIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RoomIdError {}

impl FromStr for RoomId {
    type Err = RoomIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();

        // UUID format: 8-4-4-4-12 (36 characters total with hyphens)
        if trimmed.len() != 36 {
            return Err(RoomIdError("Invalid room ID format".to_string()));
        }

        let parts: Vec<&str> = trimmed.split('-').collect();
        if parts.len() != 5 {
            return Err(RoomIdError("Invalid room ID format".to_string()));
        }

        let expected_lengths = [8, 4, 4, 4, 12];
        for (part, expected_len) in parts.iter().zip(expected_lengths.iter()) {
            if part.len() != *expected_len {
                return Err(RoomIdError("Invalid room ID format".to_string()));
            }
            if !part.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(RoomIdError("Invalid room ID format".to_string()));
            }
        }

        Ok(RoomId(trimmed.to_string()))
    }
}

impl TryFrom<String> for RoomId {
    type Error = RoomIdError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<RoomId> for String {
    fn from(r: RoomId) -> Self {
        r.0
    }
}

impl AsRef<str> for RoomId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A validated password (4-64 characters)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Password(String);

impl Password {
    /// Get the password as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner String
    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct PasswordError(String);

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PasswordError {}

impl FromStr for Password {
    type Err = PasswordError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 4 {
            return Err(PasswordError(
                "Password must be at least 4 characters".to_string(),
            ));
        }

        if s.len() > 64 {
            return Err(PasswordError(
                "Password must be at most 64 characters".to_string(),
            ));
        }

        Ok(Password(s.to_string()))
    }
}

impl TryFrom<String> for Password {
    type Error = PasswordError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Password> for String {
    fn from(p: Password) -> Self {
        p.0
    }
}

impl fmt::Display for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Password {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A validated username (2-32 alphanumeric characters)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Username(String);

impl Username {
    /// Get the username as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner String
    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct UsernameError(String);

impl fmt::Display for UsernameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UsernameError {}

impl FromStr for Username {
    type Err = UsernameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();

        if trimmed.len() < 2 {
            return Err(UsernameError(
                "Username must be at least 2 characters".to_string(),
            ));
        }

        if trimmed.len() > 32 {
            return Err(UsernameError(
                "Username must be at most 32 characters".to_string(),
            ));
        }

        if !trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(UsernameError(
                "Username must contain only letters and numbers".to_string(),
            ));
        }

        Ok(Username(trimmed.to_string()))
    }
}

impl TryFrom<String> for Username {
    type Error = UsernameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Username> for String {
    fn from(u: Username) -> Self {
        u.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberInfo {
    pub username: Username,
    pub is_online: bool,
}

/// Request for POST /api/rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    pub username: Username,
    pub password: Option<Password>,
}

/// Response from POST /api/rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    pub room_id: RoomId,
    pub owner_token: String,
    pub member_token: String,
    pub username: Username,
}

/// Response from GET /api/rooms/:id
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfoResponse {
    pub exists: bool,
    pub has_password: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_username() {
        assert!("ab".parse::<Username>().is_ok());
        assert!("jan".parse::<Username>().is_ok());
        assert!("Jan123".parse::<Username>().is_ok());
        assert!("a1b2c3".parse::<Username>().is_ok());
        assert!("A".repeat(32).parse::<Username>().is_ok());
    }

    #[test]
    fn username_too_short() {
        assert!("".parse::<Username>().is_err());
        assert!("a".parse::<Username>().is_err());
    }

    #[test]
    fn username_too_long() {
        assert!("A".repeat(33).parse::<Username>().is_err());
        assert!("A".repeat(100).parse::<Username>().is_err());
    }

    #[test]
    fn username_invalid_characters() {
        assert!("jan@".parse::<Username>().is_err());
        assert!("jan kowalski".parse::<Username>().is_err());
        assert!("jan-kowalski".parse::<Username>().is_err());
        assert!("jan_kowalski".parse::<Username>().is_err());
        assert!("jan.kowalski".parse::<Username>().is_err());
        assert!("żółć".parse::<Username>().is_err());
    }

    #[test]
    fn username_trims_whitespace() {
        let u: Username = "  jan  ".parse().unwrap();
        assert_eq!(u.as_str(), "jan");
    }

    #[test]
    fn username_display() {
        let u: Username = "jan123".parse().unwrap();
        assert_eq!(format!("{}", u), "jan123");
    }

    #[test]
    fn username_into_string() {
        let u: Username = "jan123".parse().unwrap();
        let s: String = u.into();
        assert_eq!(s, "jan123");
    }

    #[test]
    fn valid_room_id() {
        assert!(
            "550e8400-e29b-41d4-a716-446655440000"
                .parse::<RoomId>()
                .is_ok()
        );
        assert!(
            "00000000-0000-0000-0000-000000000000"
                .parse::<RoomId>()
                .is_ok()
        );
        assert!(
            "ffffffff-ffff-ffff-ffff-ffffffffffff"
                .parse::<RoomId>()
                .is_ok()
        );
        assert!(
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"
                .parse::<RoomId>()
                .is_ok()
        );
    }

    #[test]
    fn room_id_invalid_format() {
        assert!("".parse::<RoomId>().is_err());
        assert!("not-a-uuid".parse::<RoomId>().is_err());
        assert!(
            "550e8400e29b41d4a716446655440000"
                .parse::<RoomId>()
                .is_err()
        ); // no hyphens
        assert!("550e8400-e29b-41d4-a716".parse::<RoomId>().is_err()); // too short
        assert!(
            "550e8400-e29b-41d4-a716-446655440000-extra"
                .parse::<RoomId>()
                .is_err()
        ); // too long
    }

    #[test]
    fn room_id_invalid_characters() {
        assert!(
            "550e8400-e29b-41d4-a716-44665544000g"
                .parse::<RoomId>()
                .is_err()
        ); // 'g' not hex
        assert!(
            "550e8400-e29b-41d4-a716-44665544000!"
                .parse::<RoomId>()
                .is_err()
        );
    }

    #[test]
    fn room_id_display() {
        let r: RoomId = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        assert_eq!(format!("{}", r), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn room_id_into_string() {
        let r: RoomId = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        let s: String = r.into();
        assert_eq!(s, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn valid_password() {
        assert!("1234".parse::<Password>().is_ok());
        assert!("password123".parse::<Password>().is_ok());
        assert!("a".repeat(64).parse::<Password>().is_ok());
        assert!("p@ss w0rd!".parse::<Password>().is_ok());
    }

    #[test]
    fn password_too_short() {
        assert!("".parse::<Password>().is_err());
        assert!("a".parse::<Password>().is_err());
        assert!("ab".parse::<Password>().is_err());
        assert!("abc".parse::<Password>().is_err());
    }

    #[test]
    fn password_too_long() {
        assert!("a".repeat(65).parse::<Password>().is_err());
        assert!("a".repeat(100).parse::<Password>().is_err());
    }

    #[test]
    fn password_display() {
        let p: Password = "secret123".parse().unwrap();
        assert_eq!(format!("{}", p), "secret123");
    }

    #[test]
    fn password_into_string() {
        let p: Password = "secret123".parse().unwrap();
        let s: String = p.into();
        assert_eq!(s, "secret123");
    }
}
