use dioxus::prelude::*;
use inpixly_shared::MemberInfo;

#[component]
pub fn MemberList(members: Vec<MemberInfo>) -> Element {
    let online_count = members.iter().filter(|m| m.is_online).count();

    rsx! {
        div { class: "p-4",
            h3 { class: "text-gray-400 text-sm font-semibold mb-3",
                "Members ({online_count} online)"
            }
            div { class: "space-y-2 max-h-48 overflow-y-auto",
                for member in members.iter() {
                    div {
                        key: "{member.username}",
                        class: "flex items-center gap-2",
                        div {
                            class: if member.is_online {
                                "w-2 h-2 rounded-full bg-green-500"
                            } else {
                                "w-2 h-2 rounded-full bg-gray-500"
                            }
                        }
                        span {
                            class: if member.is_online {
                                "text-white"
                            } else {
                                "text-gray-500"
                            },
                            "{member.username}"
                        }
                    }
                }
            }
        }
    }
}
