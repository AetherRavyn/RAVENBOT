//! Per-conversation inbound rate limits for the native chat adapters.
//!
//! The platforms rate-limit outbound sends, but a paired chat can still spam a
//! local bot. One run per conversation is already enforced while a run is
//! active; this limiter bounds the message handling rate after a run finishes
//! and bounds pairing/command floods.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub struct ChatThrottle {
    limit: usize,
    window: Duration,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl ChatThrottle {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let Ok(mut hits) = self.hits.lock() else {
            return false;
        };
        let queue = hits.entry(key.to_string()).or_default();
        while queue
            .front()
            .is_some_and(|instant| now.duration_since(*instant) >= self.window)
        {
            queue.pop_front();
        }
        if queue.len() >= self.limit {
            return false;
        }
        queue.push_back(now);
        if hits.len() > 4_096 {
            hits.retain(|_, queue| !queue.is_empty());
        }
        true
    }
}

static TELEGRAM: OnceLock<ChatThrottle> = OnceLock::new();
static SLACK: OnceLock<ChatThrottle> = OnceLock::new();
static DISCORD: OnceLock<ChatThrottle> = OnceLock::new();
static WHATSAPP: OnceLock<ChatThrottle> = OnceLock::new();
static TEAMS: OnceLock<ChatThrottle> = OnceLock::new();

fn throttle(provider: &str) -> Option<&'static ChatThrottle> {
    let cell = match provider {
        "telegram" => &TELEGRAM,
        "slack" => &SLACK,
        "discord" => &DISCORD,
        "whatsapp" => &WHATSAPP,
        "teams" => &TEAMS,
        _ => return None,
    };
    Some(cell.get_or_init(|| ChatThrottle::new(30, Duration::from_secs(60))))
}

/// Returns false when this conversation has sent more than 30 messages in the
/// last minute. Callers silently drop the excess to avoid amplifying a flood.
pub fn allow(provider: &str, conversation: &str) -> bool {
    throttle(provider)
        .map(|throttle| throttle.allow(conversation))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_is_limited_but_other_conversations_are_not() {
        let throttle = ChatThrottle::new(2, Duration::from_secs(60));
        assert!(throttle.allow("a"));
        assert!(throttle.allow("a"));
        assert!(!throttle.allow("a"));
        assert!(throttle.allow("b"));
    }

    #[test]
    fn the_window_expires() {
        let throttle = ChatThrottle::new(1, Duration::from_millis(10));
        assert!(throttle.allow("a"));
        assert!(!throttle.allow("a"));
        std::thread::sleep(Duration::from_millis(20));
        assert!(throttle.allow("a"));
    }
}
