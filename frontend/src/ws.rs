#![allow(dead_code)]

use futures::channel::mpsc;
use inpixly_shared::WsMessage;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, MessageEvent, WebSocket};

pub struct WsClient {
    ws: WebSocket,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _on_error: Closure<dyn FnMut(JsValue)>,
}

impl WsClient {
    pub fn connect(
        url: &str,
        on_message: impl Fn(WsMessage) + 'static,
        on_close: impl Fn() + 'static,
        on_error: impl Fn(String) + 'static,
    ) -> Result<Self, String> {
        let ws = WebSocket::new(url).map_err(|e| format!("Failed to create WebSocket: {:?}", e))?;

        let on_message_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(text) = e.data().as_string() {
                match serde_json::from_str::<WsMessage>(&text) {
                    Ok(msg) => on_message(msg),
                    Err(err) => tracing::warn!("Failed to parse WsMessage: {}", err),
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        let on_close_callback = Closure::wrap(Box::new(move |_: CloseEvent| {
            on_close();
        }) as Box<dyn FnMut(CloseEvent)>);

        let on_error_callback = Closure::wrap(Box::new(move |e: JsValue| {
            on_error(format!("{:?}", e));
        }) as Box<dyn FnMut(JsValue)>);

        ws.set_onmessage(Some(on_message_callback.as_ref().unchecked_ref()));
        ws.set_onclose(Some(on_close_callback.as_ref().unchecked_ref()));
        ws.set_onerror(Some(on_error_callback.as_ref().unchecked_ref()));

        Ok(Self {
            ws,
            _on_message: on_message_callback,
            _on_close: on_close_callback,
            _on_error: on_error_callback,
        })
    }

    pub fn send(&self, msg: &WsMessage) -> Result<(), String> {
        let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        self.ws.send_with_str(&json).map_err(|e| format!("{:?}", e))
    }

    pub fn close(&self) {
        let _ = self.ws.close();
    }

    pub fn is_open(&self) -> bool {
        self.ws.ready_state() == WebSocket::OPEN
    }
}

impl Drop for WsClient {
    fn drop(&mut self) {
        let _ = self.ws.close();
    }
}

pub fn create_ws_channel(
    url: &str,
) -> Result<(WsClient, mpsc::UnboundedReceiver<WsMessage>), String> {
    let (tx, rx) = mpsc::unbounded();
    let tx = Rc::new(RefCell::new(tx));

    let tx_msg = tx.clone();
    let tx_close = tx.clone();

    let client = WsClient::connect(
        url,
        move |msg| {
            let _ = tx_msg.borrow_mut().unbounded_send(msg);
        },
        move || {
            tx_close.borrow_mut().close_channel();
        },
        |err| {
            tracing::error!("WebSocket error: {}", err);
        },
    )?;

    Ok((client, rx))
}
