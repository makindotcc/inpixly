use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use inpixly_shared::{CreateRoomRequest, CreateRoomResponse, Password, RoomInfoResponse, Username};

const API_BASE: &str = "/api";

/// Get the stored member token for a room
pub fn get_member_token(room_id: &str) -> Option<String> {
    let key = format!("room:{}:token", room_id);
    LocalStorage::get(&key).ok()
}

/// Store the member token for a room
pub fn set_member_token(room_id: &str, token: &str) {
    let key = format!("room:{}:token", room_id);
    let _ = LocalStorage::set(&key, token);
}

/// Get the stored owner token for a room
pub fn get_owner_token(room_id: &str) -> Option<String> {
    let key = format!("room:{}:owner_token", room_id);
    LocalStorage::get(&key).ok()
}

/// Store the owner token for a room
pub fn set_owner_token(room_id: &str, token: &str) {
    let key = format!("room:{}:owner_token", room_id);
    let _ = LocalStorage::set(&key, token);
}

/// Create a new room with the given username and optional password
pub async fn create_room(
    username: Username,
    password: Option<Password>,
) -> Result<CreateRoomResponse, String> {
    let request = CreateRoomRequest { username, password };

    let response = Request::post(&format!("{}/rooms", API_BASE))
        .json(&request)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        Err(format!("Failed to create room: {}", response.status()))
    }
}

/// Check if a room exists and whether it has a password
pub async fn get_room_info(room_id: &str) -> Result<RoomInfoResponse, String> {
    let response = Request::get(&format!("{}/rooms/{}", API_BASE, room_id))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        Err(format!("Failed to check room: {}", response.status()))
    }
}

/// Delete a room (requires owner token)
pub async fn delete_room(room_id: &str, owner_token: &str) -> Result<(), String> {
    let response = Request::delete(&format!("{}/rooms/{}", API_BASE, room_id))
        .header("X-Owner-Token", owner_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() || response.status() == 204 {
        Ok(())
    } else if response.status() == 403 {
        Err("Not authorized to delete this room".to_string())
    } else if response.status() == 404 {
        Err("Room not found".to_string())
    } else {
        Err(format!("Failed to delete room: {}", response.status()))
    }
}

/// Get the last used username
pub fn get_last_username() -> Option<Username> {
    LocalStorage::get("last_username").ok()
}

/// Store the last used username
pub fn set_last_username(username: &Username) {
    let _ = LocalStorage::set("last_username", username);
}

/// Get the WebSocket URL for a room
pub fn get_ws_url(room_id: &str) -> String {
    let window = web_sys::window().expect("no window");
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
    let host = location
        .host()
        .unwrap_or_else(|_| "localhost:3000".to_string());

    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    format!("{}//{}/api/rooms/{}/ws", ws_protocol, host, room_id)
}
