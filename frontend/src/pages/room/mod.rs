#![allow(deprecated)]

mod chat;
mod member_list;
mod screen_view;

use dioxus::prelude::*;
use inpixly_shared::{
    ErrorKind, JoinRequest, MemberInfo, Password, SignalingPayload, Username, WsMessage,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    MediaStream, RtcConfiguration, RtcIceCandidate, RtcIceCandidateInit, RtcPeerConnection,
    RtcPeerConnectionIceEvent, RtcSdpType, RtcSessionDescriptionInit, RtcTrackEvent,
};

use crate::api;

pub use chat::Chat;
pub use member_list::MemberList;
pub use screen_view::ScreenView;

#[derive(Clone, PartialEq)]
pub enum RoomState {
    Loading,
    NeedUsername { has_password: bool },
    NeedPassword,
    Joining,
    Connected { username: String, is_owner: bool },
    Error(String),
}

type PeerConnections = Rc<RefCell<HashMap<String, RtcPeerConnection>>>;

#[component]
pub fn Room(id: String) -> Element {
    let room_id = id.clone();
    let mut room_state = use_signal(|| RoomState::Loading);
    let members = use_signal(Vec::<MemberInfo>::new);
    let chat_messages = use_signal(Vec::<(Username, String)>::new);
    let mut username_input = use_signal(|| {
        api::get_last_username()
            .map(|u| u.to_string())
            .unwrap_or_default()
    });
    let mut password_input = use_signal(String::new);
    let mut show_password = use_signal(|| false);
    let mut room_has_password = use_signal(|| false);
    let mut username_error = use_signal(|| None::<String>);
    let ws_ref: Signal<Option<Rc<RefCell<Option<web_sys::WebSocket>>>>> = use_signal(|| None);
    let peers_ref: Signal<Option<PeerConnections>> = use_signal(|| None);
    let mut local_stream: Signal<Option<MediaStream>> = use_signal(|| None);
    let remote_streams: Signal<Vec<(String, MediaStream)>> = use_signal(Vec::new);
    let current_username: Signal<Option<String>> = use_signal(|| None);

    // Check for existing token on mount
    use_effect({
        let room_id = room_id.clone();
        move || {
            let room_id = room_id.clone();
            spawn(async move {
                match api::get_room_info(&room_id).await {
                    Ok(info) if info.exists => {
                        room_has_password.set(info.has_password);
                        if let Some(_token) = api::get_member_token(&room_id) {
                            room_state.set(RoomState::Joining);
                            connect_to_room(
                                &room_id,
                                None,
                                None,
                                room_state,
                                members,
                                chat_messages,
                                ws_ref,
                                peers_ref,
                                remote_streams,
                                current_username,
                                username_error,
                                room_has_password,
                            );
                        } else {
                            room_state.set(RoomState::NeedUsername {
                                has_password: info.has_password,
                            });
                        }
                    }
                    Ok(_) => {
                        room_state.set(RoomState::Error("Room not found".to_string()));
                    }
                    Err(e) => {
                        room_state.set(RoomState::Error(e));
                    }
                }
            });
        }
    });

    let join_room = {
        let room_id = room_id.clone();
        move |_| {
            let username_str = username_input().trim().to_string();
            let password_str = password_input().trim().to_string();

            // Validate username
            let username: Username = match username_str.parse() {
                Ok(u) => u,
                Err(e) => {
                    username_error.set(Some(e.to_string()));
                    return;
                }
            };

            // Validate password if provided
            let password: Option<Password> = if password_str.is_empty() {
                None
            } else {
                match password_str.parse() {
                    Ok(p) => Some(p),
                    Err(e) => {
                        username_error.set(Some(e.to_string()));
                        return;
                    }
                }
            };

            username_error.set(None);
            room_state.set(RoomState::Joining);
            connect_to_room(
                &room_id,
                Some(username),
                password,
                room_state,
                members,
                chat_messages,
                ws_ref,
                peers_ref,
                remote_streams,
                current_username,
                username_error,
                room_has_password,
            );
        }
    };

    let delete_room_handler = {
        let room_id = room_id.clone();
        move |_| {
            let room_id = room_id.clone();
            spawn(async move {
                if let Some(owner_token) = api::get_owner_token(&room_id) {
                    match api::delete_room(&room_id, &owner_token).await {
                        Ok(()) => {
                            room_state.set(RoomState::Error("Room deleted".to_string()));
                        }
                        Err(e) => {
                            tracing::error!("Failed to delete room: {}", e);
                        }
                    }
                }
            });
        }
    };

    let copy_link = move |_| {
        let window = web_sys::window().expect("no window");
        let location = window.location();
        if let Ok(href) = location.href() {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&href);
        }
    };

    match room_state() {
        RoomState::Loading => rsx! {
            div { class: "min-h-screen bg-gray-950 flex items-center justify-center",
                div { class: "text-white text-xl", "Loading..." }
            }
        },
        RoomState::NeedUsername { has_password } => rsx! {
            div { class: "min-h-screen bg-gray-950 flex items-center justify-center p-4",
                div { class: "bg-gray-900/90 border border-purple-900/50 rounded-lg p-8 max-w-md w-full",
                    h2 { class: "text-2xl font-bold text-white mb-6 text-center", "Join Room" }

                    if let Some(err) = username_error() {
                        div { class: "text-red-400 text-sm mb-4 text-center", "{err}" }
                    }

                    input {
                        class: "w-full bg-gray-700 text-white px-4 py-3 rounded-lg mb-4 focus:outline-none focus:ring-2 focus:ring-purple-500",
                        r#type: "text",
                        placeholder: "Enter your username",
                        value: "{username_input}",
                        oninput: move |e| username_input.set(e.value()),
                    }

                    if has_password {
                        div { class: "relative mb-4",
                            input {
                                class: "w-full bg-gray-700 text-white px-4 py-3 rounded-lg pr-12 focus:outline-none focus:ring-2 focus:ring-purple-500",
                                r#type: if show_password() { "text" } else { "password" },
                                placeholder: "Room password",
                                value: "{password_input}",
                                oninput: move |e| password_input.set(e.value()),
                            }
                            button {
                                class: "absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors",
                                r#type: "button",
                                onclick: move |_| show_password.set(!show_password()),
                                if show_password() { "Hide" } else { "Show" }
                            }
                        }
                    }

                    button {
                        class: "w-full bg-purple-600 hover:bg-purple-700 text-white font-semibold py-3 rounded-lg transition-colors",
                        onclick: join_room,
                        "Join"
                    }
                }
            }
        },
        RoomState::NeedPassword => rsx! {
            div { class: "min-h-screen bg-gray-950 flex items-center justify-center p-4",
                div { class: "bg-gray-900/90 border border-purple-900/50 rounded-lg p-8 max-w-md w-full",
                    h2 { class: "text-2xl font-bold text-white mb-6 text-center", "Password Required" }

                    if let Some(err) = username_error() {
                        div { class: "text-red-400 text-sm mb-4 text-center", "{err}" }
                    }

                    div { class: "relative mb-4",
                        input {
                            class: "w-full bg-gray-700 text-white px-4 py-3 rounded-lg pr-12 focus:outline-none focus:ring-2 focus:ring-purple-500",
                            r#type: if show_password() { "text" } else { "password" },
                            placeholder: "Room password",
                            value: "{password_input}",
                            oninput: move |e| password_input.set(e.value()),
                        }
                        button {
                            class: "absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors",
                            r#type: "button",
                            onclick: move |_| show_password.set(!show_password()),
                            if show_password() { "Hide" } else { "Show" }
                        }
                    }

                    button {
                        class: "w-full bg-purple-600 hover:bg-purple-700 text-white font-semibold py-3 rounded-lg transition-colors",
                        onclick: join_room,
                        "Join"
                    }
                }
            }
        },
        RoomState::Joining => rsx! {
            div { class: "min-h-screen bg-gray-950 flex items-center justify-center",
                div { class: "text-white text-xl", "Joining room..." }
            }
        },
        RoomState::Connected { username, is_owner } => rsx! {
            div { class: "min-h-screen bg-gray-950 flex flex-col",
                // Header
                div { class: "bg-gray-900/80 border-b border-purple-900/50 px-4 py-3 flex items-center justify-between",
                    div { class: "flex items-center gap-4",
                        Link { to: crate::Route::Home {},
                            h1 { class: "text-white font-semibold hover:text-purple-300 transition-colors cursor-pointer", "Inpixly" }
                        }
                        span { class: "text-gray-400 text-sm", "Room: {room_id}" }
                    }
                    div { class: "flex items-center gap-3",
                        span { class: "text-gray-300", "{username}" }
                        button {
                            class: "bg-gray-700 hover:bg-gray-600 text-white px-3 py-1 rounded text-sm transition-colors",
                            onclick: copy_link,
                            "Copy Link"
                        }
                        if is_owner {
                            button {
                                class: "bg-red-600 hover:bg-red-700 text-white px-3 py-1 rounded text-sm transition-colors",
                                onclick: delete_room_handler,
                                "Delete Room"
                            }
                        }
                    }
                }

                // Main content
                div { class: "flex-1 flex overflow-hidden",
                    // Screen view (main area)
                    div { class: "flex-1 p-4",
                        ScreenView {
                            local_stream: local_stream,
                            remote_streams: remote_streams,
                            on_share_start: move |stream: MediaStream| {
                                web_sys::console::log_1(&"[DEBUG] on_share_start called".into());
                                local_stream.set(Some(stream.clone()));
                                // Add stream to all peer connections
                                if let Some(peers) = peers_ref() {
                                    let peer_count = peers.borrow().len();
                                    web_sys::console::log_1(&format!("[DEBUG] Adding tracks to {} peer connections", peer_count).into());
                                    for (username, pc) in peers.borrow().iter() {
                                        let tracks = stream.get_tracks();
                                        web_sys::console::log_1(&format!("[DEBUG] Adding {} tracks to peer {}", tracks.length(), username).into());
                                        for i in 0..tracks.length() {
                                            let track = tracks.get(i);
                                            if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                                                if let Ok(add_track_fn) = js_sys::Reflect::get(pc, &"addTrack".into()) {
                                                    if let Ok(func) = add_track_fn.dyn_into::<js_sys::Function>() {
                                                        let result = func.call2(pc, &track, &stream);
                                                        web_sys::console::log_1(&format!("[DEBUG] addTrack result: {:?}", result.is_ok()).into());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    web_sys::console::log_1(&"[DEBUG] No peers_ref available!".into());
                                }
                            },
                            on_share_stop: move |_| {
                                // Remove tracks from all peer connections
                                if let Some(peers) = peers_ref() {
                                    for (_username, pc) in peers.borrow().iter() {
                                        // Get all senders and remove them
                                        let senders = pc.get_senders();
                                        for i in 0..senders.length() {
                                            if let Some(sender) = senders.get(i).dyn_ref::<web_sys::RtcRtpSender>() {
                                                let _ = pc.remove_track(sender);
                                            }
                                        }
                                    }
                                }
                                local_stream.set(None);
                            },
                        }
                    }

                    // Sidebar
                    div { class: "w-80 bg-gray-900/80 border-l border-purple-900/50 flex flex-col",
                        div { class: "border-b border-purple-900/50",
                            MemberList { members: members() }
                        }
                        div { class: "flex-1 overflow-hidden",
                            Chat {
                                messages: chat_messages(),
                                on_send: move |msg: String| {
                                    if let Some(ws_rc) = ws_ref() {
                                        if let Some(ws) = ws_rc.borrow().as_ref() {
                                            let chat_msg = WsMessage::ChatMessage { message: msg };
                                            if let Ok(json) = serde_json::to_string(&chat_msg) {
                                                let _ = ws.send_with_str(&json);
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        },
        RoomState::Error(msg) => rsx! {
            div { class: "min-h-screen bg-gray-950 flex items-center justify-center p-4",
                div { class: "text-center",
                    div { class: "text-red-400 text-xl mb-4", "{msg}" }
                    a {
                        href: "/",
                        class: "text-purple-400 hover:text-purple-300 underline",
                        "Go back home"
                    }
                }
            }
        },
    }
}

fn connect_to_room(
    room_id: &str,
    username: Option<Username>,
    password: Option<Password>,
    mut room_state: Signal<RoomState>,
    mut members: Signal<Vec<MemberInfo>>,
    mut chat_messages: Signal<Vec<(Username, String)>>,
    mut ws_ref: Signal<Option<Rc<RefCell<Option<web_sys::WebSocket>>>>>,
    mut peers_ref: Signal<Option<PeerConnections>>,
    mut remote_streams: Signal<Vec<(String, MediaStream)>>,
    mut current_username: Signal<Option<String>>,
    mut username_error: Signal<Option<String>>,
    room_has_password: Signal<bool>,
) {
    let url = api::get_ws_url(room_id);
    let room_id = room_id.to_string();

    let ws = match web_sys::WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(e) => {
            room_state.set(RoomState::Error(format!("Failed to connect: {:?}", e)));
            return;
        }
    };

    let ws_rc = Rc::new(RefCell::new(Some(ws.clone())));
    ws_ref.set(Some(ws_rc.clone()));

    let peers: PeerConnections = Rc::new(RefCell::new(HashMap::new()));
    peers_ref.set(Some(peers.clone()));

    let room_id_for_open = room_id.clone();
    let room_id_for_msg = room_id.clone();
    let username_for_open = username.clone();
    let password_for_open = password.clone();
    let ws_for_signaling = ws_rc.clone();
    let peers_for_msg = peers.clone();

    let onopen = Closure::wrap(Box::new(move |_: JsValue| {
        let join_msg = if let Some(uname) = username_for_open.clone() {
            WsMessage::Join(JoinRequest::WithUsername {
                username: uname,
                password: password_for_open.clone(),
            })
        } else if let Some(token) = api::get_member_token(&room_id_for_open) {
            WsMessage::Join(JoinRequest::WithToken { token })
        } else {
            room_state.set(RoomState::NeedUsername {
                has_password: room_has_password(),
            });
            return;
        };

        if let Some(ref ws_opt) = *ws_rc.borrow() {
            if let Ok(json) = serde_json::to_string(&join_msg) {
                let _ = ws_opt.send_with_str(&json);
            }
        }
    }) as Box<dyn FnMut(JsValue)>);

    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Some(text) = e.data().as_string() {
            match serde_json::from_str::<WsMessage>(&text) {
                Ok(WsMessage::JoinedAs {
                    username,
                    token,
                    is_owner,
                }) => {
                    api::set_member_token(&room_id_for_msg, &token);
                    api::set_last_username(&username);
                    let username_str = username.to_string();
                    current_username.set(Some(username_str.clone()));
                    room_state.set(RoomState::Connected {
                        username: username_str,
                        is_owner,
                    });
                }
                Ok(WsMessage::MemberList { members: m }) => {
                    // Create peer connections for all online members
                    let my_username = current_username();
                    for member in m.iter() {
                        let member_str = member.username.to_string();
                        if member.is_online && my_username.as_deref() != Some(member_str.as_str()) {
                            create_peer_connection_and_offer(
                                &member_str,
                                peers_for_msg.clone(),
                                ws_for_signaling.clone(),
                                remote_streams,
                            );
                        }
                    }
                    members.set(m);
                }
                Ok(WsMessage::MemberJoined { username }) => {
                    members.with_mut(|list| {
                        if !list.iter().any(|m| m.username == username) {
                            list.push(MemberInfo {
                                username: username.clone(),
                                is_online: true,
                            });
                        } else {
                            for m in list.iter_mut() {
                                if m.username == username {
                                    m.is_online = true;
                                }
                            }
                        }
                    });
                    // Create offer for new member
                    let my_username = current_username();
                    let username_str = username.to_string();
                    if my_username.as_deref() != Some(username_str.as_str()) {
                        create_peer_connection_and_offer(
                            &username_str,
                            peers_for_msg.clone(),
                            ws_for_signaling.clone(),
                            remote_streams,
                        );
                    }
                }
                Ok(WsMessage::MemberLeft { username }) => {
                    members.with_mut(|list| {
                        for m in list.iter_mut() {
                            if m.username == username {
                                m.is_online = false;
                            }
                        }
                    });
                    let username_str = username.to_string();
                    // Close peer connection
                    if let Some(pc) = peers_for_msg.borrow_mut().remove(&username_str) {
                        pc.close();
                    }
                    // Remove remote stream
                    remote_streams.with_mut(|streams| {
                        streams.retain(|(u, _)| u != &username_str);
                    });
                }
                Ok(WsMessage::SignalingMessage { from, payload }) => {
                    web_sys::console::log_1(
                        &format!(
                            "[DEBUG] Received signaling message from {}: {:?}",
                            from,
                            match &payload {
                                SignalingPayload::Offer { .. } => "Offer",
                                SignalingPayload::Answer { .. } => "Answer",
                                SignalingPayload::IceCandidate { .. } => "IceCandidate",
                            }
                        )
                        .into(),
                    );
                    handle_signaling_message(
                        &from,
                        payload,
                        peers_for_msg.clone(),
                        ws_for_signaling.clone(),
                        remote_streams,
                    );
                }
                Ok(WsMessage::Error(ErrorKind::TokenNotFound)) => {
                    room_state.set(RoomState::NeedUsername {
                        has_password: room_has_password(),
                    });
                }
                Ok(WsMessage::Error(ErrorKind::InvalidUsername { message })) => {
                    username_error.set(Some(message));
                    room_state.set(RoomState::NeedUsername {
                        has_password: room_has_password(),
                    });
                }
                Ok(WsMessage::Error(ErrorKind::UsernameTaken)) => {
                    username_error.set(Some(
                        "Username is taken. Please choose a different one.".to_string(),
                    ));
                    room_state.set(RoomState::NeedUsername {
                        has_password: room_has_password(),
                    });
                }
                Ok(WsMessage::Error(ErrorKind::PasswordRequired)) => {
                    username_error.set(Some("Password is required for this room.".to_string()));
                    room_state.set(RoomState::NeedPassword);
                }
                Ok(WsMessage::Error(ErrorKind::IncorrectPassword)) => {
                    username_error.set(Some("Incorrect password.".to_string()));
                    room_state.set(RoomState::NeedPassword);
                }
                Ok(WsMessage::Error(ErrorKind::RoomNotFound)) => {
                    room_state.set(RoomState::Error("Room not found".to_string()));
                }
                Ok(WsMessage::Error(ErrorKind::Other { message })) => {
                    room_state.set(RoomState::Error(message));
                }
                Ok(WsMessage::Chat { from, message }) => {
                    chat_messages.with_mut(|msgs| {
                        msgs.push((from, message));
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Failed to parse message: {}", e);
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    let onclose = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
        if matches!(room_state(), RoomState::Connected { .. }) {
            room_state.set(RoomState::Error("Connection lost".to_string()));
        }
    }) as Box<dyn FnMut(web_sys::CloseEvent)>);

    let onerror = Closure::wrap(Box::new(move |_: JsValue| {
        room_state.set(RoomState::Error("WebSocket error".to_string()));
    }) as Box<dyn FnMut(JsValue)>);

    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

    onopen.forget();
    onmessage.forget();
    onclose.forget();
    onerror.forget();
}

fn create_peer_connection_and_offer(
    remote_username: &str,
    peers: PeerConnections,
    ws: Rc<RefCell<Option<web_sys::WebSocket>>>,
    mut remote_streams: Signal<Vec<(String, MediaStream)>>,
) {
    let remote = remote_username.to_string();

    // Don't create if already exists
    if peers.borrow().contains_key(&remote) {
        return;
    }

    let config = RtcConfiguration::new();
    let ice_servers = js_sys::Array::new();
    let server = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &server,
        &"urls".into(),
        &"stun:stun.l.google.com:19302".into(),
    );
    ice_servers.push(&server);
    config.set_ice_servers(&ice_servers);

    let pc = match RtcPeerConnection::new_with_configuration(&config) {
        Ok(pc) => pc,
        Err(_) => return,
    };

    // ICE candidate handler
    let ws_ice = ws.clone();
    let remote_ice = remote.clone();
    let on_ice = Closure::wrap(Box::new(move |e: RtcPeerConnectionIceEvent| {
        if let Some(candidate) = e.candidate() {
            let msg = WsMessage::IceCandidate {
                to: remote_ice.clone(),
                candidate: candidate.candidate(),
            };
            if let Some(ws) = ws_ice.borrow().as_ref() {
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = ws.send_with_str(&json);
                }
            }
        }
    }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
    pc.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));
    on_ice.forget();

    // Track handler
    let remote_track = remote.clone();
    let on_track = Closure::wrap(Box::new(move |e: RtcTrackEvent| {
        web_sys::console::log_1(&format!("[DEBUG] on_track fired for {}", remote_track).into());
        let streams = e.streams();
        web_sys::console::log_1(&format!("[DEBUG] streams.length() = {}", streams.length()).into());
        if streams.length() > 0 {
            if let Ok(stream) = streams.get(0).dyn_into::<MediaStream>() {
                let username = remote_track.clone();
                web_sys::console::log_1(
                    &format!("[DEBUG] Adding remote stream for {}", username).into(),
                );

                // Set up onended handler for the track
                let track = e.track();
                let username_for_ended = username.clone();
                let on_ended = Closure::wrap(Box::new(move |_: JsValue| {
                    web_sys::console::log_1(
                        &format!("[DEBUG] Track ended for {}", username_for_ended).into(),
                    );
                    let username = username_for_ended.clone();
                    remote_streams.with_mut(|list| {
                        list.retain(|(u, _)| u != &username);
                    });
                }) as Box<dyn FnMut(JsValue)>);
                track.set_onended(Some(on_ended.as_ref().unchecked_ref()));
                on_ended.forget();

                remote_streams.with_mut(|list| {
                    // Remove existing stream for this user
                    list.retain(|(u, _)| u != &username);
                    list.push((username, stream));
                });
                web_sys::console::log_1(
                    &format!(
                        "[DEBUG] remote_streams now has {} entries",
                        remote_streams().len()
                    )
                    .into(),
                );
            }
        } else {
            web_sys::console::log_1(&"[DEBUG] No streams in track event!".into());
        }
    }) as Box<dyn FnMut(RtcTrackEvent)>);
    pc.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
    on_track.forget();

    // Negotiation needed handler - triggers when tracks are added
    let ws_neg = ws.clone();
    let remote_neg = remote.clone();
    let pc_neg = pc.clone();
    let on_negotiation = Closure::wrap(Box::new(move |_: JsValue| {
        web_sys::console::log_1(
            &format!("[DEBUG] onnegotiationneeded fired for {}", remote_neg).into(),
        );
        let ws_inner = ws_neg.clone();
        let remote_inner = remote_neg.clone();
        let pc_inner = pc_neg.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(offer) = JsFuture::from(pc_inner.create_offer()).await {
                if let Some(sdp) = js_sys::Reflect::get(&offer, &"sdp".into())
                    .ok()
                    .and_then(|v| v.as_string())
                {
                    let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
                    desc.sdp(&sdp);
                    if JsFuture::from(pc_inner.set_local_description(&desc))
                        .await
                        .is_ok()
                    {
                        web_sys::console::log_1(
                            &format!("[DEBUG] Sending renegotiation offer to {}", remote_inner)
                                .into(),
                        );
                        let msg = WsMessage::Offer {
                            to: remote_inner,
                            sdp,
                        };
                        if let Some(ws) = ws_inner.borrow().as_ref() {
                            if let Ok(json) = serde_json::to_string(&msg) {
                                let _ = ws.send_with_str(&json);
                            }
                        }
                    }
                }
            }
        });
    }) as Box<dyn FnMut(JsValue)>);
    pc.set_onnegotiationneeded(Some(on_negotiation.as_ref().unchecked_ref()));
    on_negotiation.forget();

    peers.borrow_mut().insert(remote.clone(), pc.clone());

    // Create and send offer
    let ws_offer = ws.clone();
    let remote_offer = remote.clone();
    let pc_offer = pc.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(offer) = JsFuture::from(pc_offer.create_offer()).await {
            if let Some(sdp) = js_sys::Reflect::get(&offer, &"sdp".into())
                .ok()
                .and_then(|v| v.as_string())
            {
                let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
                desc.sdp(&sdp);
                if JsFuture::from(pc_offer.set_local_description(&desc))
                    .await
                    .is_ok()
                {
                    let msg = WsMessage::Offer {
                        to: remote_offer,
                        sdp,
                    };
                    if let Some(ws) = ws_offer.borrow().as_ref() {
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = ws.send_with_str(&json);
                        }
                    }
                }
            }
        }
    });
}

fn handle_signaling_message(
    from: &str,
    payload: SignalingPayload,
    peers: PeerConnections,
    ws: Rc<RefCell<Option<web_sys::WebSocket>>>,
    mut remote_streams: Signal<Vec<(String, MediaStream)>>,
) {
    let from = from.to_string();

    match payload {
        SignalingPayload::Offer { sdp } => {
            // Create peer connection if not exists
            if !peers.borrow().contains_key(&from) {
                let config = RtcConfiguration::new();
                let ice_servers = js_sys::Array::new();
                let server = js_sys::Object::new();
                let _ = js_sys::Reflect::set(
                    &server,
                    &"urls".into(),
                    &"stun:stun.l.google.com:19302".into(),
                );
                ice_servers.push(&server);
                config.set_ice_servers(&ice_servers);

                if let Ok(pc) = RtcPeerConnection::new_with_configuration(&config) {
                    // ICE handler
                    let ws_ice = ws.clone();
                    let remote_ice = from.clone();
                    let on_ice = Closure::wrap(Box::new(move |e: RtcPeerConnectionIceEvent| {
                        if let Some(candidate) = e.candidate() {
                            let msg = WsMessage::IceCandidate {
                                to: remote_ice.clone(),
                                candidate: candidate.candidate(),
                            };
                            if let Some(ws) = ws_ice.borrow().as_ref() {
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    let _ = ws.send_with_str(&json);
                                }
                            }
                        }
                    })
                        as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
                    pc.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));
                    on_ice.forget();

                    // Track handler
                    let remote_track = from.clone();
                    let on_track = Closure::wrap(Box::new(move |e: RtcTrackEvent| {
                        web_sys::console::log_1(
                            &format!(
                                "[DEBUG] on_track (from offer handler) fired for {}",
                                remote_track
                            )
                            .into(),
                        );
                        let streams = e.streams();
                        web_sys::console::log_1(
                            &format!("[DEBUG] streams.length() = {}", streams.length()).into(),
                        );
                        if streams.length() > 0 {
                            if let Ok(stream) = streams.get(0).dyn_into::<MediaStream>() {
                                let username = remote_track.clone();
                                web_sys::console::log_1(
                                    &format!("[DEBUG] Adding remote stream for {}", username)
                                        .into(),
                                );

                                // Set up onended handler for the track
                                let track = e.track();
                                let username_for_ended = username.clone();
                                let on_ended = Closure::wrap(Box::new(move |_: JsValue| {
                                    web_sys::console::log_1(
                                        &format!("[DEBUG] Track ended for {}", username_for_ended)
                                            .into(),
                                    );
                                    let username = username_for_ended.clone();
                                    remote_streams.with_mut(|list| {
                                        list.retain(|(u, _)| u != &username);
                                    });
                                })
                                    as Box<dyn FnMut(JsValue)>);
                                track.set_onended(Some(on_ended.as_ref().unchecked_ref()));
                                on_ended.forget();

                                remote_streams.with_mut(|list| {
                                    list.retain(|(u, _)| u != &username);
                                    list.push((username, stream));
                                });
                                web_sys::console::log_1(
                                    &format!(
                                        "[DEBUG] remote_streams now has {} entries",
                                        remote_streams().len()
                                    )
                                    .into(),
                                );
                            }
                        } else {
                            web_sys::console::log_1(
                                &"[DEBUG] No streams in track event (from offer handler)!".into(),
                            );
                        }
                    })
                        as Box<dyn FnMut(RtcTrackEvent)>);
                    pc.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
                    on_track.forget();

                    // Negotiation needed handler
                    let ws_neg = ws.clone();
                    let remote_neg = from.clone();
                    let pc_neg = pc.clone();
                    let on_negotiation = Closure::wrap(Box::new(move |_: JsValue| {
                        let ws_inner = ws_neg.clone();
                        let remote_inner = remote_neg.clone();
                        let pc_inner = pc_neg.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(offer) = JsFuture::from(pc_inner.create_offer()).await {
                                if let Some(sdp) = js_sys::Reflect::get(&offer, &"sdp".into())
                                    .ok()
                                    .and_then(|v| v.as_string())
                                {
                                    let mut desc =
                                        RtcSessionDescriptionInit::new(RtcSdpType::Offer);
                                    desc.sdp(&sdp);
                                    if JsFuture::from(pc_inner.set_local_description(&desc))
                                        .await
                                        .is_ok()
                                    {
                                        let msg = WsMessage::Offer {
                                            to: remote_inner,
                                            sdp,
                                        };
                                        if let Some(ws) = ws_inner.borrow().as_ref() {
                                            if let Ok(json) = serde_json::to_string(&msg) {
                                                let _ = ws.send_with_str(&json);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    })
                        as Box<dyn FnMut(JsValue)>);
                    pc.set_onnegotiationneeded(Some(on_negotiation.as_ref().unchecked_ref()));
                    on_negotiation.forget();

                    peers.borrow_mut().insert(from.clone(), pc);
                }
            }

            // Handle offer
            let pc = peers.borrow().get(&from).cloned();
            if let Some(pc) = pc {
                let ws_answer = ws.clone();
                let from_answer = from.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
                    desc.sdp(&sdp);
                    if JsFuture::from(pc.set_remote_description(&desc))
                        .await
                        .is_ok()
                    {
                        if let Ok(answer) = JsFuture::from(pc.create_answer()).await {
                            if let Some(answer_sdp) = js_sys::Reflect::get(&answer, &"sdp".into())
                                .ok()
                                .and_then(|v| v.as_string())
                            {
                                let mut local_desc =
                                    RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                                local_desc.sdp(&answer_sdp);
                                if JsFuture::from(pc.set_local_description(&local_desc))
                                    .await
                                    .is_ok()
                                {
                                    let msg = WsMessage::Answer {
                                        to: from_answer,
                                        sdp: answer_sdp,
                                    };
                                    if let Some(ws) = ws_answer.borrow().as_ref() {
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let _ = ws.send_with_str(&json);
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
        SignalingPayload::Answer { sdp } => {
            if let Some(pc) = peers.borrow().get(&from).cloned() {
                wasm_bindgen_futures::spawn_local(async move {
                    let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
                    desc.sdp(&sdp);
                    let _ = JsFuture::from(pc.set_remote_description(&desc)).await;
                });
            }
        }
        SignalingPayload::IceCandidate { candidate } => {
            if let Some(pc) = peers.borrow().get(&from).cloned() {
                wasm_bindgen_futures::spawn_local(async move {
                    let mut init = RtcIceCandidateInit::new(&candidate);
                    init.sdp_mid(Some("0"));
                    init.sdp_m_line_index(Some(0));
                    if let Ok(candidate) = RtcIceCandidate::new(&init) {
                        let _ = JsFuture::from(
                            pc.add_ice_candidate_with_opt_rtc_ice_candidate(Some(&candidate)),
                        )
                        .await;
                    }
                });
            }
        }
    }
}
