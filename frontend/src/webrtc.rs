#![allow(dead_code)]
#![allow(deprecated)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inpixly_shared::{SignalingPayload, WsMessage};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    MediaStream, RtcConfiguration, RtcDataChannel, RtcDataChannelInit, RtcIceCandidate,
    RtcIceCandidateInit, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType,
    RtcSessionDescriptionInit, RtcTrackEvent,
};

/// ICE servers for WebRTC connection
const ICE_SERVERS: &[&str] = &["stun:stun.l.google.com:19302", "stun:stun1.l.google.com:19302"];

/// Manages WebRTC peer connections for a room
pub struct PeerManager {
    local_username: String,
    peers: HashMap<String, PeerConnection>,
    on_track: Rc<RefCell<Box<dyn Fn(String, MediaStream)>>>,
    on_chat_message: Rc<RefCell<Box<dyn Fn(String, String)>>>,
    send_signaling: Rc<RefCell<Box<dyn Fn(WsMessage)>>>,
}

impl PeerManager {
    pub fn new(
        local_username: String,
        on_track: impl Fn(String, MediaStream) + 'static,
        on_chat_message: impl Fn(String, String) + 'static,
        send_signaling: impl Fn(WsMessage) + 'static,
    ) -> Self {
        Self {
            local_username,
            peers: HashMap::new(),
            on_track: Rc::new(RefCell::new(Box::new(on_track))),
            on_chat_message: Rc::new(RefCell::new(Box::new(on_chat_message))),
            send_signaling: Rc::new(RefCell::new(Box::new(send_signaling))),
        }
    }

    /// Create a new peer connection and initiate the offer
    pub async fn create_offer(&mut self, remote_username: &str) -> Result<(), String> {
        let pc = self.get_or_create_peer(remote_username)?;

        // Create offer
        let offer = JsFuture::from(pc.connection.create_offer())
            .await
            .map_err(|e| format!("Failed to create offer: {:?}", e))?;

        let offer_sdp = js_sys::Reflect::get(&offer, &"sdp".into())
            .map_err(|_| "No SDP in offer")?
            .as_string()
            .ok_or("SDP is not a string")?;

        // Set local description
        let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        desc.sdp(&offer_sdp);
        JsFuture::from(pc.connection.set_local_description(&desc))
            .await
            .map_err(|e| format!("Failed to set local description: {:?}", e))?;

        // Send offer via signaling
        let msg = WsMessage::Offer {
            to: remote_username.to_string(),
            sdp: offer_sdp,
        };
        (self.send_signaling.borrow())(msg);

        Ok(())
    }

    /// Handle an incoming offer
    pub async fn handle_offer(&mut self, from: &str, sdp: &str) -> Result<(), String> {
        let pc = self.get_or_create_peer(from)?;

        // Set remote description
        let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        desc.sdp(sdp);
        JsFuture::from(pc.connection.set_remote_description(&desc))
            .await
            .map_err(|e| format!("Failed to set remote description: {:?}", e))?;

        // Create answer
        let answer = JsFuture::from(pc.connection.create_answer())
            .await
            .map_err(|e| format!("Failed to create answer: {:?}", e))?;

        let answer_sdp = js_sys::Reflect::get(&answer, &"sdp".into())
            .map_err(|_| "No SDP in answer")?
            .as_string()
            .ok_or("SDP is not a string")?;

        // Set local description
        let mut local_desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        local_desc.sdp(&answer_sdp);
        JsFuture::from(pc.connection.set_local_description(&local_desc))
            .await
            .map_err(|e| format!("Failed to set local description: {:?}", e))?;

        // Send answer via signaling
        let msg = WsMessage::Answer {
            to: from.to_string(),
            sdp: answer_sdp,
        };
        (self.send_signaling.borrow())(msg);

        Ok(())
    }

    /// Handle an incoming answer
    pub async fn handle_answer(&mut self, from: &str, sdp: &str) -> Result<(), String> {
        if let Some(pc) = self.peers.get(from) {
            let mut desc = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
            desc.sdp(sdp);
            JsFuture::from(pc.connection.set_remote_description(&desc))
                .await
                .map_err(|e| format!("Failed to set remote description: {:?}", e))?;
        }
        Ok(())
    }

    /// Handle an incoming ICE candidate
    pub async fn handle_ice_candidate(&self, from: &str, candidate: &str) -> Result<(), String> {
        if let Some(pc) = self.peers.get(from) {
            let mut init = RtcIceCandidateInit::new(candidate);
            init.sdp_mid(Some("0"));
            init.sdp_m_line_index(Some(0));

            let candidate = RtcIceCandidate::new(&init)
                .map_err(|e| format!("Failed to create ICE candidate: {:?}", e))?;

            JsFuture::from(
                pc.connection
                    .add_ice_candidate_with_opt_rtc_ice_candidate(Some(&candidate)),
            )
            .await
            .map_err(|e| format!("Failed to add ICE candidate: {:?}", e))?;
        }
        Ok(())
    }

    /// Handle signaling message from server
    pub async fn handle_signaling(
        &mut self,
        from: &str,
        payload: SignalingPayload,
    ) -> Result<(), String> {
        match payload {
            SignalingPayload::Offer { sdp } => self.handle_offer(from, &sdp).await,
            SignalingPayload::Answer { sdp } => self.handle_answer(from, &sdp).await,
            SignalingPayload::IceCandidate { candidate } => {
                self.handle_ice_candidate(from, &candidate).await
            }
        }
    }

    /// Add a local media stream to all peer connections
    pub fn add_stream(&self, stream: &MediaStream) {
        for pc in self.peers.values() {
            for track in stream.get_tracks() {
                if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                    // Use JS interop to call addTrack
                    if let Ok(add_track_fn) = js_sys::Reflect::get(&pc.connection, &"addTrack".into())
                    {
                        if let Ok(func) = add_track_fn.dyn_into::<js_sys::Function>() {
                            let _ = func.call2(&pc.connection, &track, stream);
                        }
                    }
                }
            }
        }
    }

    /// Send a chat message to all peers
    pub fn send_chat_message(&self, message: &str) {
        for pc in self.peers.values() {
            if let Some(ref channel) = pc.data_channel {
                // Check ready state via JS
                let state = js_sys::Reflect::get(channel, &"readyState".into())
                    .ok()
                    .and_then(|v| v.as_string());
                if state.as_deref() == Some("open") {
                    let _ = channel.send_with_str(message);
                }
            }
        }
    }

    /// Close all peer connections
    pub fn close_all(&mut self) {
        for (_, pc) in self.peers.drain() {
            pc.connection.close();
        }
    }

    fn get_or_create_peer(&mut self, remote_username: &str) -> Result<&mut PeerConnection, String> {
        if !self.peers.contains_key(remote_username) {
            let pc = PeerConnection::new(
                remote_username,
                &self.local_username,
                self.on_track.clone(),
                self.on_chat_message.clone(),
                self.send_signaling.clone(),
            )?;
            self.peers.insert(remote_username.to_string(), pc);
        }
        Ok(self.peers.get_mut(remote_username).unwrap())
    }
}

/// A single WebRTC peer connection
struct PeerConnection {
    connection: RtcPeerConnection,
    data_channel: Option<RtcDataChannel>,
    _on_ice_candidate: Closure<dyn FnMut(RtcPeerConnectionIceEvent)>,
    _on_track: Closure<dyn FnMut(RtcTrackEvent)>,
    _on_datachannel: Closure<dyn FnMut(web_sys::RtcDataChannelEvent)>,
}

impl PeerConnection {
    fn new(
        remote_username: &str,
        _local_username: &str,
        on_track: Rc<RefCell<Box<dyn Fn(String, MediaStream)>>>,
        on_chat_message: Rc<RefCell<Box<dyn Fn(String, String)>>>,
        send_signaling: Rc<RefCell<Box<dyn Fn(WsMessage)>>>,
    ) -> Result<Self, String> {
        // Create RTCConfiguration with ICE servers
        let config = RtcConfiguration::new();
        let ice_servers = js_sys::Array::new();
        for server in ICE_SERVERS {
            let server_obj = js_sys::Object::new();
            js_sys::Reflect::set(&server_obj, &"urls".into(), &(*server).into())
                .map_err(|_| "Failed to set ICE server URL")?;
            ice_servers.push(&server_obj);
        }
        config.set_ice_servers(&ice_servers);

        let pc = RtcPeerConnection::new_with_configuration(&config)
            .map_err(|e| format!("Failed to create peer connection: {:?}", e))?;

        // Create data channel for chat
        let mut dc_init = RtcDataChannelInit::new();
        dc_init.ordered(true);
        let data_channel = pc.create_data_channel_with_data_channel_dict("chat", &dc_init);

        // Setup data channel message handler
        let on_chat = on_chat_message.clone();
        let remote = remote_username.to_string();
        let on_dc_message = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
            if let Some(text) = e.data().as_string() {
                (on_chat.borrow())(remote.clone(), text);
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        data_channel.set_onmessage(Some(on_dc_message.as_ref().unchecked_ref()));
        on_dc_message.forget();

        // ICE candidate handler
        let remote_for_ice = remote_username.to_string();
        let send_sig = send_signaling.clone();
        let on_ice_candidate = Closure::wrap(Box::new(move |e: RtcPeerConnectionIceEvent| {
            if let Some(candidate) = e.candidate() {
                let msg = WsMessage::IceCandidate {
                    to: remote_for_ice.clone(),
                    candidate: candidate.candidate(),
                };
                (send_sig.borrow())(msg);
            }
        }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
        pc.set_onicecandidate(Some(on_ice_candidate.as_ref().unchecked_ref()));

        // Track handler
        let remote_for_track = remote_username.to_string();
        let on_track_closure = Closure::wrap(Box::new(move |e: RtcTrackEvent| {
            let streams = e.streams();
            if streams.length() > 0 {
                let stream_js = streams.get(0);
                if let Ok(stream) = stream_js.dyn_into::<MediaStream>() {
                    (on_track.borrow())(remote_for_track.clone(), stream);
                }
            }
        }) as Box<dyn FnMut(RtcTrackEvent)>);
        pc.set_ontrack(Some(on_track_closure.as_ref().unchecked_ref()));

        // Data channel handler (for incoming channels)
        let on_chat_dc = on_chat_message.clone();
        let remote_for_dc = remote_username.to_string();
        let on_datachannel = Closure::wrap(Box::new(move |e: web_sys::RtcDataChannelEvent| {
            let channel = e.channel();
            let remote = remote_for_dc.clone();
            let on_chat = on_chat_dc.clone();
            let on_msg = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                if let Some(text) = e.data().as_string() {
                    (on_chat.borrow())(remote.clone(), text);
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>);
            channel.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
            on_msg.forget();
        }) as Box<dyn FnMut(web_sys::RtcDataChannelEvent)>);
        pc.set_ondatachannel(Some(on_datachannel.as_ref().unchecked_ref()));

        Ok(Self {
            connection: pc,
            data_channel: Some(data_channel),
            _on_ice_candidate: on_ice_candidate,
            _on_track: on_track_closure,
            _on_datachannel: on_datachannel,
        })
    }
}
