use crate::room::Room;
use inpixly_shared::RoomId;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

pub type Rooms = Arc<RwLock<HashMap<RoomId, Room>>>;

#[derive(Clone)]
pub struct AppState {
    pub rooms: Rooms,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
