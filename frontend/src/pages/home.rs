use dioxus::prelude::*;
use inpixly_shared::{Password, Username};

use crate::api;
use crate::Route;

#[component]
pub fn Home() -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut creating = use_signal(|| false);
    let mut username_input = use_signal(|| {
        api::get_last_username()
            .map(|u| u.to_string())
            .unwrap_or_default()
    });
    let mut password_input = use_signal(String::new);
    let mut show_password = use_signal(|| false);
    let navigator = use_navigator();

    let mut do_create_room = move || {
        let username_str = username_input().trim().to_string();
        let password_str = password_input().trim().to_string();

        // Validate username
        let username: Username = match username_str.parse() {
            Ok(u) => u,
            Err(e) => {
                error.set(Some(e.to_string()));
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
                    error.set(Some(e.to_string()));
                    return;
                }
            }
        };

        let nav = navigator.clone();
        spawn(async move {
            creating.set(true);
            error.set(None);

            match api::create_room(username, password).await {
                Ok(response) => {
                    let room_id = response.room_id.to_string();
                    // Store tokens
                    api::set_owner_token(&room_id, &response.owner_token);
                    api::set_member_token(&room_id, &response.member_token);
                    api::set_last_username(&response.username);
                    // Navigate to room
                    nav.push(Route::Room { id: room_id });
                }
                Err(e) => {
                    error.set(Some(e));
                    creating.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "min-h-screen bg-gray-950 flex items-center justify-center p-4",
            div { class: "max-w-md w-full text-center",
                h1 { class: "text-5xl font-bold text-white mb-4", "Inpixly" }
                p { class: "text-purple-300 mb-8 text-lg",
                    "Low-latency screen sharing for everyone"
                }

                div { class: "bg-gray-900/90 border border-purple-900/50 rounded-lg p-6 mb-6",
                    input {
                        class: "w-full bg-gray-700 text-white px-4 py-3 rounded-lg mb-4 focus:outline-none focus:ring-2 focus:ring-purple-500",
                        r#type: "text",
                        placeholder: "Enter your username",
                        value: "{username_input}",
                        oninput: move |e| username_input.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                do_create_room();
                            }
                        },
                    }

                    // Password input with toggle
                    div { class: "relative mb-4",
                        input {
                            class: "w-full bg-gray-700 text-white px-4 py-3 rounded-lg pr-12 focus:outline-none focus:ring-2 focus:ring-purple-500",
                            r#type: if show_password() { "text" } else { "password" },
                            placeholder: "Room password (optional)",
                            value: "{password_input}",
                            oninput: move |e| password_input.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    do_create_room();
                                }
                            },
                        }
                        button {
                            class: "absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors",
                            r#type: "button",
                            onclick: move |_| show_password.set(!show_password()),
                            if show_password() {
                                "Hide"
                            } else {
                                "Show"
                            }
                        }
                    }

                    if let Some(err) = error() {
                        p { class: "text-red-400 text-sm mb-4", "{err}" }
                    }

                    button {
                        class: "w-full bg-purple-600 hover:bg-purple-700 text-white font-semibold py-3 rounded-lg transition-colors text-lg disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: creating(),
                        onclick: move |_| do_create_room(),
                        if creating() {
                            "Creating..."
                        } else {
                            "Create Room"
                        }
                    }
                }

                div { class: "text-purple-400 text-sm",
                    p { "Share your screen with ultra-low latency" }
                    p { "No sign-up required" }
                }
            }
        }
    }
}
