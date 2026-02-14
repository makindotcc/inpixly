use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlVideoElement, MediaStream};

#[component]
pub fn ScreenView(
    local_stream: Signal<Option<MediaStream>>,
    remote_streams: Signal<Vec<(String, MediaStream)>>,
    on_share_start: EventHandler<MediaStream>,
    on_share_stop: EventHandler<()>,
) -> Element {
    let mut is_sharing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Effect to attach local stream to video element
    // Uses spawn_local with timeout to ensure DOM is updated before attaching
    use_effect(move || {
        let stream_opt = local_stream();

        if stream_opt.is_none() {
            return;
        }

        wasm_bindgen_futures::spawn_local(async move {
            // Wait for DOM to be updated
            TimeoutFuture::new(0).await;

            if let Some(stream) = stream_opt {
                let window = web_sys::window().expect("no window");
                let document = window.document().expect("no document");
                if let Some(video) = document.get_element_by_id("local-screen") {
                    if let Ok(video) = video.dyn_into::<HtmlVideoElement>() {
                        video.set_src_object(Some(&stream));
                        video.set_muted(true);
                        if let Ok(promise) = video.play() {
                            let _ = JsFuture::from(promise).await;
                        }
                    }
                }
            }
        });
    });

    // Effect to attach remote streams to video elements
    // Uses spawn_local with timeout to ensure DOM is updated before attaching
    use_effect(move || {
        let streams = remote_streams();

        if streams.is_empty() {
            return;
        }

        // Defer to next event loop tick to ensure DOM is updated after render
        wasm_bindgen_futures::spawn_local(async move {
            // Wait for DOM to be updated
            TimeoutFuture::new(0).await;

            let window = web_sys::window().expect("no window");
            let document = window.document().expect("no document");

            for (username, stream) in streams.iter() {
                let video_id = format!("remote-screen-{}", username);
                if let Some(video) = document.get_element_by_id(&video_id) {
                    if let Ok(video) = video.dyn_into::<HtmlVideoElement>() {
                        video.set_src_object(Some(stream));
                        video.set_muted(true);
                        // Play the video
                        if let Ok(promise) = video.play() {
                            let _ = JsFuture::from(promise).await;
                        }
                    }
                }
            }
        });
    });

    let start_sharing = move |_| {
        spawn(async move {
            match start_screen_capture().await {
                Ok(stream) => {
                    is_sharing.set(true);
                    error.set(None);
                    on_share_start.call(stream);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
        });
    };

    let stop_sharing = move |_| {
        if let Some(stream) = local_stream() {
            for track in stream.get_tracks() {
                if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
        }
        is_sharing.set(false);
        on_share_stop.call(());
    };

    let remote = remote_streams();

    rsx! {
        div { class: "h-full flex flex-col",
            // Controls
            div { class: "mb-4 flex gap-3",
                if !is_sharing() {
                    button {
                        class: "bg-purple-600 hover:bg-purple-700 text-white font-semibold py-2 px-4 rounded transition-colors",
                        onclick: start_sharing,
                        "Share Screen"
                    }
                } else {
                    button {
                        class: "bg-red-600 hover:bg-red-700 text-white font-semibold py-2 px-4 rounded transition-colors",
                        onclick: stop_sharing,
                        "Stop Sharing"
                    }
                }
            }

            if let Some(err) = error() {
                div { class: "text-red-400 mb-4", "{err}" }
            }

            // Video grid
            div { class: "flex-1 bg-gray-900/50 border border-purple-900/30 rounded-lg overflow-hidden grid gap-2 p-2",
                style: "grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));",

                // Local screen
                if is_sharing() {
                    div { class: "relative bg-gray-950 rounded overflow-hidden",
                        video {
                            id: "local-screen",
                            class: "w-full h-full object-contain",
                            autoplay: true,
                            muted: true,
                            playsinline: true,
                        }
                        div { class: "absolute bottom-2 left-2 bg-black/50 px-2 py-1 rounded text-white text-sm",
                            "You (sharing)"
                        }
                    }
                }

                // Remote screens
                for (username, _stream) in remote.iter() {
                    div {
                        key: "{username}",
                        class: "relative bg-gray-950 rounded overflow-hidden",
                        video {
                            id: "remote-screen-{username}",
                            class: "w-full h-full object-contain",
                            autoplay: true,
                            muted: true,
                            playsinline: true,
                        }
                        div { class: "absolute bottom-2 left-2 bg-black/50 px-2 py-1 rounded text-white text-sm",
                            "{username}"
                        }
                    }
                }

                // Placeholder when no screens
                if !is_sharing() && remote.is_empty() {
                    div { class: "flex items-center justify-center text-gray-500 text-center p-8 col-span-full",
                        div {
                            p { class: "text-lg mb-2", "No screen shared" }
                            p { class: "text-sm", "Click 'Share Screen' to start sharing" }
                        }
                    }
                }
            }
        }
    }
}

async fn start_screen_capture() -> Result<MediaStream, String> {
    let window = web_sys::window().ok_or("No window")?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices().map_err(|_| "No media devices")?;

    let constraints = web_sys::DisplayMediaStreamConstraints::new();

    let promise = media_devices
        .get_display_media_with_constraints(&constraints)
        .map_err(|_| "Failed to get display media")?;

    let result = JsFuture::from(promise)
        .await
        .map_err(|e| format!("Failed to capture screen: {:?}", e))?;

    result
        .dyn_into::<MediaStream>()
        .map_err(|_| "Not a MediaStream".to_string())
}
