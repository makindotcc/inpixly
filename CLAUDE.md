# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Inpixly is a self-hosted video conferencing solution similar to Google Meet, with a primary focus on low-latency screen sharing. It supports both browser and desktop clients, with the desktop app offering higher quality screen capture.

## Tech Stack

- **Server**: Axum (Rust web framework)
- **Frontend**: Dioxus (Rust UI framework)
  - Web: Dioxus web renderer
  - Desktop: Dioxus with native renderer (Blitz)
- **Real-time Communication**: WebRTC for low-latency screen sharing and video

## Build Commands

```bash
# Build the server
cargo build -p inpixly-server

# Run the server
cargo run -p inpixly-server

# Build in release mode
cargo build --release
```

## Architecture

The project is organized as a Cargo workspace with separate crates:
- `server/` - Axum-based signaling server and API backend

## Code Style

- Use `anyhow::Result<T>` instead of importing `Result` from anyhow
- Don't use obvious comments. preferably never. Use smaller functions.