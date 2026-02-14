use dioxus::prelude::*;
use inpixly_shared::Username;

#[component]
pub fn Chat(messages: Vec<(Username, String)>, on_send: EventHandler<String>) -> Element {
    let mut input = use_signal(String::new);

    let send_message = move |_| {
        let msg = input().trim().to_string();
        if !msg.is_empty() {
            on_send.call(msg);
            input.set(String::new());
        }
    };

    rsx! {
        div { class: "flex flex-col h-full",
            // Chat header
            div { class: "p-4 border-b border-purple-900/50",
                h3 { class: "text-gray-400 text-sm font-semibold", "Chat" }
            }

            // Messages
            div { class: "flex-1 overflow-y-auto p-4 space-y-3",
                for (i, (username, message)) in messages.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: "text-sm",
                        span { class: "text-purple-400 font-semibold", "{username}: " }
                        span { class: "text-gray-300", "{message}" }
                    }
                }
                if messages.is_empty() {
                    div { class: "text-gray-500 text-sm text-center",
                        "No messages yet"
                    }
                }
            }

            // Input
            div { class: "p-4 border-t border-purple-900/50",
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 bg-gray-700 text-white px-3 py-2 rounded text-sm focus:outline-none focus:ring-2 focus:ring-purple-500",
                        r#type: "text",
                        placeholder: "Type a message...",
                        value: "{input}",
                        oninput: move |e| input.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                let msg = input().trim().to_string();
                                if !msg.is_empty() {
                                    on_send.call(msg);
                                    input.set(String::new());
                                }
                            }
                        },
                    }
                    button {
                        class: "bg-purple-600 hover:bg-purple-700 text-white px-4 py-2 rounded text-sm transition-colors",
                        onclick: send_message,
                        "Send"
                    }
                }
            }
        }
    }
}
