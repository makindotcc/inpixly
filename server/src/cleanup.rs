use std::time::Duration;

use chrono::Utc;
use tracing::info;

use crate::state::AppState;

/// Room inactivity threshold (30 days)
const ROOM_INACTIVE_DAYS: i64 = 30;

const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Spawns a background task that periodically removes inactive rooms
pub fn spawn_cleanup_task(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CLEANUP_INTERVAL).await;
            cleanup_inactive_rooms(&state).await;
        }
    });
}

async fn cleanup_inactive_rooms(state: &AppState) {
    let now = Utc::now();
    let threshold = chrono::Duration::days(ROOM_INACTIVE_DAYS);

    let mut rooms = state.rooms.write().await;
    let initial_count = rooms.len();

    rooms.retain(|room_id, room| {
        let age = now.signed_duration_since(room.last_activity);
        if age > threshold {
            info!(
                "Removing inactive room {} (inactive for {} days)",
                room_id,
                age.num_days()
            );
            false
        } else {
            true
        }
    });

    let removed = initial_count - rooms.len();
    if removed > 0 {
        info!("Cleanup complete: removed {} inactive rooms", removed);
    }
}
