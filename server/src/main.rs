mod cleanup;
mod config;
mod room;
mod state;
mod ws;

use std::net::SocketAddr;

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    routing::{get, post},
};
use config::Config;
use state::AppState;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing::info;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use inpixly_shared::{CreateRoomRequest, CreateRoomResponse, ErrorKind, RoomId, RoomInfoResponse};

use crate::room::Room;

/// POST /api/rooms - Create a new room
async fn create_room(
    State(state): State<AppState>,
    Json(request): Json<CreateRoomRequest>,
) -> Result<Json<CreateRoomResponse>, (StatusCode, Json<ErrorKind>)> {
    let mut room = Room::new(request.password);
    let room_id = room.id.clone();
    let owner_token = room.owner_token.clone();

    // Add creator as first member
    let (username, member_token) = room
        .add_member(request.username, false)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(e)))?;

    let mut rooms = state.rooms.write().await;
    rooms.insert(room_id.clone(), room);

    info!("Created new room: {} by {}", room_id, username);

    Ok(Json(CreateRoomResponse {
        room_id,
        owner_token,
        member_token,
        username,
    }))
}

/// GET /api/rooms/:id - Check if room exists
async fn get_room(
    Path(room_id): Path<RoomId>,
    State(state): State<AppState>,
) -> Json<RoomInfoResponse> {
    let rooms = state.rooms.read().await;
    match rooms.get(&room_id) {
        Some(room) => Json(RoomInfoResponse {
            exists: true,
            has_password: room.has_password(),
        }),
        None => Json(RoomInfoResponse {
            exists: false,
            has_password: false,
        }),
    }
}

/// DELETE /api/rooms/:id - Delete a room (requires owner_token)
async fn delete_room(
    Path(room_id): Path<RoomId>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> StatusCode {
    let owner_token = match headers.get("X-Owner-Token") {
        Some(token) => match token.to_str() {
            Ok(t) => t.to_string(),
            Err(_) => return StatusCode::BAD_REQUEST,
        },
        None => return StatusCode::UNAUTHORIZED,
    };

    let mut rooms = state.rooms.write().await;

    if let Some(room) = rooms.get(&room_id) {
        if room.is_owner(&owner_token) {
            rooms.remove(&room_id);
            info!("Deleted room: {}", room_id);
            StatusCode::NO_CONTENT
        } else {
            StatusCode::FORBIDDEN
        }
    } else {
        StatusCode::NOT_FOUND
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let config = Config::load("config.nix").await?;

    let state = AppState::new();

    // Spawn cleanup task
    cleanup::spawn_cleanup_task(state.clone());

    // CORS configuration for frontend
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/api/rooms", post(create_room))
        .route("/api/rooms/{id}", get(get_room).delete(delete_room))
        .route("/api/rooms/{id}/ws", get(ws::ws_handler))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .context("failed to bind to address")?;
    info!("Server listening on http://{}", config.server.bind);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("server error")?;

    Ok(())
}
