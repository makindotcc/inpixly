use crate::{
    room::RoomEvent,
    state::{AppState, Rooms},
};
use axum::{
    extract::{
        ConnectInfo, Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use futures_util::{Sink, SinkExt, StreamExt};
use inpixly_shared::{ErrorKind, JoinRequest, RoomId, Username, WsMessage};
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
use tokio::{select, sync::broadcast, time::timeout};
use tracing::{debug, error, info, info_span, warn};

type WsSender = futures_util::stream::SplitSink<WebSocket, Message>;
type WsReceiver = futures_util::stream::SplitStream<WebSocket>;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(room_id): Path<RoomId>,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, room_id, state, addr))
}

#[tracing::instrument(skip(socket, state), fields(room_id = %room_id, client_addr = %addr))]
async fn handle_socket(mut socket: WebSocket, room_id: RoomId, state: AppState, addr: SocketAddr) {
    {
        let rooms = state.rooms.read().await;
        if !rooms.contains_key(&room_id) {
            send_ws_error(&mut socket, ErrorKind::RoomNotFound).await;
            return;
        }
    }
    let Some(mut member) = handshake_with_timeout(&state, &mut socket, &room_id).await else {
        return;
    };
    let (mut sender, mut ws_reader) = socket.split();
    let member_span = info_span!("member", username = %member.username);

    info!(?member_span, "User joined room.");

    loop {
        select! {
            biased;
            event = member.room_broadcast_rx.recv() => match event {
                Ok(RoomEvent::Kick { token, success }) if token == member.token => {
                    info!("Received kick event for this member.");
                    let _ = sender.send(Message::Close(None)).await;
                    sender.close().await.ok();
                    if let Some(disconnect_guard) = success.lock().unwrap().take() {
                        member.disconnect(disconnect_guard);
                    }
                    break;
                }
                Ok(RoomEvent::Kick { .. }) => {}
                Ok(RoomEvent::Broadcast(ws_msg)) => {
                    if matches!(&ws_msg, WsMessage::MemberJoined { username } if username == &member.username) {
                        continue;
                    }
                    send_ws_json(&mut sender, &ws_msg).await;
                }
                // Ok(ws_msg) => {
                //     // Force disconnect if another tab took over this session
                //     if matches!(&ws_msg, WsMessage::ForceDisconnect { token } if token == &member.token) {
                //         info!("Force disconnecting due to new connection from another tab.");
                //         if let Ok(json) = serde_json::to_string(&ws_msg) {
                //             let _ = sender.send(Message::Text(json.into())).await;
                //         }
                //         send_ws_json(&mut sender, &WsMessage::Error(ErrorKind::TokenTaken)).await;
                //         break;
                //     }

                //     // Don't send MemberJoined to the member who just joined (they already know)
                //     if matches!(&ws_msg, WsMessage::MemberJoined { username } if username == &member.username) {
                //         continue;
                //     }

                //     // Don't forward ForceDisconnect for other users
                //     if matches!(&ws_msg, WsMessage::ForceDisconnect { .. }) {
                //         continue;
                //     }

                //     if let Ok(json) = serde_json::to_string(&ws_msg) {
                //         if sender.send(Message::Text(json.into())).await.is_err() {
                //             break;
                //         }
                //     }
                // }
                Err(_) => break,
            },
            msg = ws_reader.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) if text.len() > 30_000 => {
                        warn!("Received excessively long message, disconnecting.");
                        break;
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_client_message(&text, &room_id, &member.username, &state).await {
                            error!("Error handling client message: {}", e);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(Message::Pong(_)))| Some(Ok(Message::Ping(_))) => {
                        continue;
                    }
                    msg => {
                        warn!("Invalid WebSocket message: {msg:?}");
                        break;
                    }
                }
            }
        }
    }
}

struct WsMember {
    rooms: Rooms,
    room_id: RoomId,
    room_broadcast_rx: broadcast::Receiver<RoomEvent>,
    token: String,
    username: Username,
    is_owner: bool,
    disconnect_token: Option<tokio_util::sync::DropGuard>,
}

impl WsMember {
    pub fn disconnect(mut self, disconnect_token: tokio_util::sync::DropGuard) {
        self.disconnect_token = Some(disconnect_token);
    }
}

impl Drop for WsMember {
    fn drop(&mut self) {
        let rooms = self.rooms.clone();
        let room_id = self.room_id.clone();
        let token = self.token.clone();
        let disconnect_token = self.disconnect_token.take();

        tokio::spawn(async move {
            let mut rooms = rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.on_disconnect(&token, disconnect_token);
            }
        });
    }
}

async fn handshake_with_timeout(
    state: &AppState,
    socket: &mut WebSocket,
    room_id: &RoomId,
) -> Option<WsMember> {
    const JOIN_TIMEOUT: Duration = Duration::from_secs(10);

    match timeout(JOIN_TIMEOUT, handshake(state, socket, room_id)).await {
        Ok(result) => result,
        Err(_) => {
            warn!("Join timeout exceeded.");
            send_ws_error(socket, ErrorKind::JoinTimeout).await;
            None
        }
    }
}

async fn handshake(state: &AppState, socket: &mut WebSocket, room_id: &RoomId) -> Option<WsMember> {
    loop {
        let text = match socket.next().await {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Binary(_))) => {
                warn!("Received unexpected binary message, disconnecting.");
                return None;
            }
            Some(Ok(Message::Close(_))) | None => {
                return None;
            }
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {
                continue;
            }
            Some(Err(e)) => {
                warn!("WebSocket error during handshake: {}", e);
                return None;
            }
        };

        let join_request = match serde_json::from_str(&text) {
            Ok(WsMessage::Join(join_request)) => join_request,
            Ok(other) => {
                warn!("Expected Join message during handshake, got: {:?}", other);
                continue;
            }
            Err(err) => {
                warn!(
                    "Failed to parse WebSocket message during handshake: {}",
                    err
                );
                return None;
            }
        };

        let member = match join_room(state, room_id, socket, join_request, None).await {
            Ok(member) => member,
            Err(error) => {
                send_ws_error(socket, error).await;
                return None;
            }
        };

        return Some(member);
    }
}

async fn join_room(
    state: &AppState,
    room_id: &RoomId,
    socket: &mut WebSocket,
    request: JoinRequest,
    terminate_old_session_token: Option<tokio_util::sync::CancellationToken>,
) -> Result<WsMember, ErrorKind> {
    if let Some(token) = &terminate_old_session_token {
        token.cancelled().await;
    }

    let mut rooms = state.rooms.write().await;
    let room = rooms.get_mut(room_id).ok_or(ErrorKind::RoomNotFound)?;

    let member = match request {
        JoinRequest::WithToken { ref token } => {
            debug!("Attempting token-based login for token: {}", token);
            let username = match room.login_member(token) {
                Ok(username) => username,
                Err(ErrorKind::TokenAlreadyInUse) if terminate_old_session_token.is_none() => {
                    let Some(disconnect_token) = room.force_logout_member(token) else {
                        return Err(ErrorKind::TokenAlreadyInUse);
                    };
                    drop(rooms);
                    return Box::pin(join_room(
                        state,
                        room_id,
                        socket,
                        request,
                        Some(disconnect_token),
                    ))
                    .await;
                }
                Err(e) => return Err(e),
            };
            let is_owner = room.is_owner(token);
            WsMember {
                rooms: Arc::clone(&state.rooms),
                room_broadcast_rx: room.broadcast_tx.subscribe(),
                room_id: room_id.clone(),
                token: token.clone(),
                username,
                is_owner,
                disconnect_token: None,
            }
        }
        JoinRequest::WithUsername { username, password } => {
            room.verify_password(password.as_ref())?;
            let (username, token) = room.add_member(username, true)?;
            WsMember {
                rooms: Arc::clone(&state.rooms),
                room_broadcast_rx: room.broadcast_tx.subscribe(),
                room_id: room_id.clone(),
                token,
                username,
                is_owner: false,
                disconnect_token: None,
            }
        }
    };

    let messages = [
        WsMessage::JoinedAs {
            username: member.username.clone(),
            token: member.token.clone(),
            is_owner: member.is_owner,
        },
        WsMessage::MemberList {
            members: room.get_member_list(),
        },
    ];
    drop(rooms);

    for msg in &messages {
        send_ws_json(socket, msg).await;
    }
    Ok(member)
}

async fn send_ws_error(sender: &mut (impl Sink<Message> + Unpin), error_kind: ErrorKind) {
    send_ws_json(sender, &WsMessage::Error(error_kind)).await;
}

async fn send_ws_json(sender: &mut (impl Sink<Message> + Unpin), msg: &WsMessage) {
    match serde_json::to_string(msg) {
        Ok(json) => {
            let _ = sender.send(Message::Text(json.into())).await;
        }
        Err(err) => {
            error!("Failed to serialize message '{msg:?}': {err}");
        }
    }
}

// // async fn handle_join(
// //     state: &AppState,
// //     room_id: &RoomId,
// //     request: JoinRequest,
// // ) -> Result<WsMember, ErrorKind> {
// //     let mut rooms = state.rooms.write().await;
// //     let room = rooms.get_mut(room_id).ok_or(ErrorKind::RoomNotFound)?;

// //     match request {
// //         JoinRequest::WithToken { token } => {
// //             // Check if member exists and if already online
// //             let was_online = room
// //                 .find_member_by_token_mut(&token)
// //                 .map(|m| m.online)
// //                 .ok_or(ErrorKind::TokenNotFound)?;

// //             // If already online, force disconnect the old connection
// //             if was_online {
// //                 room.broadcast(WsMessage::ForceDisconnect {
// //                     token: token.clone(),
// //                 });
// //             }

// //             // Now update the member
// //             let member = room.find_member_by_token_mut(&token).unwrap();
// //             member.online = true;
// //             member.last_seen = chrono::Utc::now();
// //             let username = member.username.clone();
// //             let is_owner = room.is_owner(&token);
// //             room.touch();
// //             Ok(WsMember {
// //                 rooms: Arc::clone(&state.rooms),
// //                 room_id: room_id.clone(),
// //                 token,
// //                 username,
// //                 is_owner,
// //             })
// //         }
// //         JoinRequest::WithUsername { username, password } => {
// //             room.verify_password(password.as_ref())?;
// //             let (assigned_username, token) = room.add_member(username)?;
// //             let is_owner = room.is_owner(&token);
// //             Ok(WsMember {
// //                 rooms: Arc::clone(&state.rooms),
// //                 room_id: room_id.clone(),
// //                 token,
// //                 username: assigned_username,
// //                 is_owner,
// //             })
// //         }
// //     }
// // }

async fn handle_client_message(
    text: &str,
    room_id: &RoomId,
    username: &Username,
    state: &AppState,
) -> anyhow::Result<()> {
    let msg: WsMessage = serde_json::from_str(text)?;

    match msg {
        WsMessage::Offer { to, sdp } => {
            // forward_signaling(
            //     room_id,
            //     from_username,
            //     &to,
            //     SignalingPayload::Offer { sdp },
            //     state,
            // )
            // .await;
        }
        WsMessage::Answer { to, sdp } => {
            // forward_signaling(
            //     room_id,
            //     from_username,
            //     &to,
            //     SignalingPayload::Answer { sdp },
            //     state,
            // )
            // .await;
        }
        WsMessage::IceCandidate { to, candidate } => {
            // forward_signaling(
            //     room_id,
            //     from_username,
            //     &to,
            //     SignalingPayload::IceCandidate { candidate },
            //     state,
            // )
            // .await;
        }
        WsMessage::ChatMessage { message } => {
            let rooms = state.rooms.read().await;
            if let Some(room) = rooms.get(room_id) {
                room.broadcast(RoomEvent::Broadcast(WsMessage::Chat {
                    from: username.clone(),
                    message,
                }));
            }
        }
        WsMessage::Leave => {
            // Handled by connection close
        }
        _ => {
            // Ignore other message types from client
        }
    }

    Ok(())
}

// // async fn forward_signaling(
// //     room_id: &RoomId,
// //     from: &str,
// //     _to: &str,
// //     payload: SignalingPayload,
// //     state: &AppState,
// // ) {
// //     let rooms = state.rooms.read().await;
// //     if let Some(room) = rooms.get(room_id) {
// //         // Broadcast signaling message - recipient will filter by 'to' field
// //         room.broadcast(WsMessage::SignalingMessage {
// //             from: from.to_string(),
// //             payload,
// //         });
// //     }
// // }
