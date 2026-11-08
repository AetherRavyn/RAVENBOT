//! Cross-agent model request coordination.
//!
//! An office dispatches every ready task at once. That is the right shape for
//! the graph — the tasks are independent — but it is the wrong shape for a
//! provider: seven agents started seven simultaneous requests, the provider
//! answered all seven with `429 FreeUsageLimitError`, and the run failed with
//! seven copies of the same message. Retrying each one harder is not a fix.
//!
//! This governor is shared by every run in the process and gives each provider:
//!
//!  - a **concurrency cap** (three requests in flight by default),
//!  - a **minimum gap** between request starts, so a burst is spread out,
//!  - a **global cooldown** after a 429, so every waiting agent pauses for the
//!    retry window instead of each discovering the limit on its own.
//!
//! It deliberately sits in the runtime, not in each provider: custom providers,
//! built-ins and the office all go through `call_model`, so none of them can
//! bypass it by accident.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Requests allowed in flight per provider.
const MAX_IN_FLIGHT: usize = 3;
/// Minimum spacing between two request starts for the same provider.
const MIN_GAP: Duration = Duration::from_millis(150);

struct Gate {
    permits: Arc<Semaphore>,
    /// The next instant a request may start. Advanced under the mutex so two
    /// callers cannot claim the same slot.
    next_slot: Mutex<Instant>,
    /// Set after a 429. Every caller waits this out before asking again.
    cooldown_until: Mutex<Option<Instant>>,
}

impl Gate {
    fn new() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
            next_slot: Mutex::new(Instant::now()),
            cooldown_until: Mutex::new(None),
        }
    }

    fn cooldown_wait(&self) -> Duration {
        let guard = self
            .cooldown_until
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or_default()
    }

    fn claim_slot(&self) -> Duration {
        let mut next = self.next_slot.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let wait = next.saturating_duration_since(now);
        *next = now.max(*next) + MIN_GAP;
        wait
    }

    fn penalize(&self, retry_after: Duration) {
        let until = Instant::now() + retry_after;
        let mut guard = self
            .cooldown_until
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard = Some(match *guard {
            Some(existing) => existing.max(until),
            None => until,
        });
    }
}

#[derive(Default)]
pub struct ProviderGovernor {
    gates: Mutex<HashMap<String, Arc<Gate>>>,
}

impl ProviderGovernor {
    fn gate(&self, key: &str) -> Arc<Gate> {
        let mut gates = self.gates.lock().unwrap_or_else(|e| e.into_inner());
        gates
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Gate::new()))
            .clone()
    }

    /// Wait for this provider's turn and return a permit held for the duration
    /// of one request.
    ///
    /// Order: the shared cooldown first (a 429 is not worth waking up early
    /// for), then this caller's spaced slot, then the concurrency permit. The
    /// permit is acquired last so a caller waiting on a slot is not occupying
    /// one of the three in-flight places.
    pub async fn acquire(&self, key: &str) -> OwnedSemaphorePermit {
        let gate = self.gate(key);
        loop {
            let wait = gate.cooldown_wait();
            if wait.is_zero() {
                break;
            }
            tokio::time::sleep(wait).await;
        }
        let slot = gate.claim_slot();
        if !slot.is_zero() {
            tokio::time::sleep(slot).await;
        }
        gate.permits
            .clone()
            .acquire_owned()
            .await
            .expect("provider semaphore is never closed")
    }

    /// Pause every request to this provider for `retry_after`.
    pub fn penalize(&self, key: &str, retry_after: Duration) {
        self.gate(key).penalize(retry_after);
    }

    /// How long a provider is currently paused for, for diagnostics/tests.
    pub fn cooldown_remaining(&self, key: &str) -> Duration {
        self.gate(key).cooldown_wait()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_penalty_pauses_the_whole_provider() {
        let governor = ProviderGovernor::default();
        assert!(governor.cooldown_remaining("OpenCodeGo").is_zero());
        governor.penalize("OpenCodeGo", Duration::from_millis(80));
        assert!(governor.cooldown_remaining("OpenCodeGo") > Duration::from_millis(20));
        // Another provider is untouched — a 429 from Go must not stall a
        // parallel Anthropic chat.
        assert!(governor.cooldown_remaining("Anthropic").is_zero());
    }

    #[tokio::test]
    async fn concurrent_requests_are_capped() {
        let governor = Arc::new(ProviderGovernor::default());
        let in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..12 {
            let g = governor.clone();
            let n = in_flight.clone();
            let p = peak.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = g.acquire("OpenCodeGo").await;
                let now = n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                p.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                n.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        assert!(
            peak.load(std::sync::atomic::Ordering::SeqCst) <= MAX_IN_FLIGHT,
            "more than {MAX_IN_FLIGHT} requests reached the provider at once"
        );
    }
}
