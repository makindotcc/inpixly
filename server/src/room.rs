use chrono::{DateTime, Utc};
use inpixly_shared::{ErrorKind, MemberInfo, Password, RoomId, Username, WsMessage};
use std::{collections::BTreeMap, str::FromStr, sync::Arc};
use subtle::ConstantTimeEq;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::info;
use uuid::Uuid;

pub struct Room {
    pub id: RoomId,
    pub owner_token: String,
    pub password: Option<Password>,
    pub members: BTreeMap<MemberToken, Member>,
    pub last_activity: DateTime<Utc>,
    pub broadcast_tx: broadcast::Sender<RoomEvent>,
}

impl Room {
    pub fn new(password: Option<Password>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            id: Uuid::new_v4().to_string().parse().unwrap(),
            owner_token: Uuid::new_v4().to_string(),
            password,
            members: BTreeMap::new(),
            last_activity: Utc::now(),
            broadcast_tx,
        }
    }

    /// Check if room has a password
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }

    /// Verify the provided password (uses constant-time comparison)
    pub fn verify_password(&self, password: Option<&Password>) -> Result<(), ErrorKind> {
        match (&self.password, password) {
            (None, _) => Ok(()), // No password required
            (Some(_), None) => Err(ErrorKind::PasswordRequired),
            (Some(room_pass), Some(provided_pass)) => {
                let room_bytes = room_pass.as_str().as_bytes();
                let provided_bytes = provided_pass.as_str().as_bytes();
                if room_bytes.ct_eq(provided_bytes).into() {
                    Ok(())
                } else {
                    Err(ErrorKind::IncorrectPassword)
                }
            }
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }

    /// Check if a username is already taken
    fn is_username_taken(&self, username: &Username) -> bool {
        self.members.values().any(|m| &m.username == username)
    }

    /// Generate a unique username by adding numeric suffix if needed
    /// Returns None if no unique username can be found within limits
    fn generate_unique_username(&self, base_username: Username) -> Option<Username> {
        if !self.is_username_taken(&base_username) {
            return Some(base_username.clone());
        }

        for counter in 1..100 {
            let Ok(candidate) = Username::from_str(&format!("{}{}", base_username, counter)) else {
                break;
            };
            if !self.is_username_taken(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    /// Add a new member with a username, returns the assigned username and token
    /// Returns Err if no unique username can be assigned
    pub fn add_member(
        &mut self,
        requested_username: Username,
        is_online: bool,
    ) -> Result<(Username, MemberToken), ErrorKind> {
        let username = self
            .generate_unique_username(requested_username)
            .ok_or(ErrorKind::UsernameTaken)?;

        let member = Member::new(username.clone(), is_online);
        let token = member.token.clone();
        self.members.insert(token.clone(), member);
        let _ = self
            .broadcast_tx
            .send(RoomEvent::Broadcast(WsMessage::MemberJoined {
                username: username.clone(),
            }));
        self.touch();
        Ok((username, token))
    }

    /// Handle member login by token, returns the username if successful.
    pub fn login_member(&mut self, token: &str) -> Result<Username, ErrorKind> {
        let Some(member) = self.members.get_mut(token) else {
            return Err(ErrorKind::TokenNotFound);
        };
        if member.is_online {
            return Err(ErrorKind::TokenAlreadyInUse);
        }
        member.set_online(true);
        let _ = self
            .broadcast_tx
            .send(RoomEvent::Broadcast(WsMessage::MemberJoined {
                username: member.username.clone(),
            }));
        let username = member.username.clone();
        self.touch();
        Ok(username)
    }

    pub fn force_logout_member(&mut self, token: &str) -> Option<CancellationToken> {
        let Some(member) = self.members.get_mut(token) else {
            return None;
        };
        if !member.is_online {
            return None;
        }
        member.set_online(false);

        let disconnect_token = CancellationToken::new();
        let _ = self.broadcast_tx.send(RoomEvent::Kick {
            token: token.to_string(),
            success: Arc::new(std::sync::Mutex::new(Some(
                disconnect_token.clone().drop_guard(),
            ))),
        });
        let _ = self
            .broadcast_tx
            .send(RoomEvent::Broadcast(WsMessage::MemberLeft {
                username: member.username.clone(),
            }));
        self.touch();
        Some(disconnect_token)
    }

    /// Handle member disconnection
    pub fn on_disconnect(
        &mut self,
        token: &str,
        disconnect_token: Option<tokio_util::sync::DropGuard>,
    ) {
        if let Some(member) = self.members.get_mut(token) {
            info!(
                room_id = %self.id,
                username = %member.username(),
                "User left room."
            );
            member.set_online(false);
            let _ = self
                .broadcast_tx
                .send(RoomEvent::Broadcast(WsMessage::MemberLeft {
                    username: member.username.clone(),
                }));
            self.touch();
        }
        drop(disconnect_token);
    }

    /// Get list of all members
    pub fn get_member_list(&self) -> Vec<MemberInfo> {
        self.members.values().map(|m| m.to_info()).collect()
    }

    /// Check if the provided token is the owner token
    pub fn is_owner(&self, token: &str) -> bool {
        self.owner_token == token
    }

    /// Broadcast a room event to all subscribers
    pub fn broadcast(&self, msg: RoomEvent) {
        let _ = self.broadcast_tx.send(msg);
    }

    /// Subscribe to room events
    pub fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.broadcast_tx.subscribe()
    }
}

impl Default for Room {
    fn default() -> Self {
        Self::new(None)
    }
}

#[derive(Clone)]
pub enum RoomEvent {
    Broadcast(WsMessage),
    Kick {
        token: String,
        success: Arc<std::sync::Mutex<Option<tokio_util::sync::DropGuard>>>,
    },
}

pub type MemberToken = String;

#[derive(Debug, Clone)]
pub struct Member {
    username: Username,
    token: MemberToken,
    last_seen: DateTime<Utc>,
    is_online: bool,
}

impl Member {
    pub fn new(username: Username, is_online: bool) -> Self {
        Self {
            username,
            token: Uuid::new_v4().to_string(),
            is_online,
            last_seen: Utc::now(),
        }
    }

    pub fn to_info(&self) -> MemberInfo {
        MemberInfo {
            username: self.username.clone(),
            is_online: self.is_online,
        }
    }

    pub fn set_online(&mut self, online: bool) {
        self.is_online = online;
        self.last_seen = Utc::now();
    }

    pub fn username(&self) -> &Username {
        &self.username
    }
}
