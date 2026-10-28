use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use daroyan_protocol::message::ApiMessage;
use tokio::sync::mpsc::Sender;

#[derive(Clone, Default)]
pub struct Agents {
    sockets: Arc<Mutex<HashMap<String, AgentSocket>>>,
    next_generation: Arc<AtomicU64>,
}

struct AgentSocket {
    generation: u64,
    sender: Sender<ApiMessage>,
}

impl Agents {
    pub fn register(&self, server_id: &str, sender: Sender<ApiMessage>) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        self.lock()
            .insert(server_id.to_string(), AgentSocket { generation, sender });
        generation
    }

    pub fn unregister(&self, server_id: &str, generation: u64) -> bool {
        let mut sockets = self.lock();
        let is_current = sockets
            .get(server_id)
            .is_some_and(|socket| socket.generation == generation);
        if is_current {
            sockets.remove(server_id);
        }
        is_current
    }

    pub fn disconnect(&self, server_id: &str) {
        self.lock().remove(server_id);
    }

    pub fn disconnect_all(&self) {
        self.lock().clear();
    }

    pub fn is_connected(&self, server_id: &str) -> bool {
        self.lock().contains_key(server_id)
    }

    pub fn send(&self, server_id: &str, message: ApiMessage) -> bool {
        self.lock()
            .get(server_id)
            .is_some_and(|socket| socket.sender.try_send(message).is_ok())
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, AgentSocket>> {
        self.sockets.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
