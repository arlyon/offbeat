use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

#[derive(Default)]
struct State {
    closed: bool,
    handles: Vec<JoinHandle<()>>,
}

/// Owns detached tasks so security-sensitive lifecycle transitions can join them.
#[derive(Clone, Default)]
pub struct TaskScope {
    state: Arc<Mutex<State>>,
}

impl TaskScope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a task unless shutdown has begun.
    pub fn spawn<F>(&self, future: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return false;
        }
        state.handles.retain(|handle| !handle.is_finished());
        state.handles.push(tokio::spawn(future));
        true
    }

    /// Prevent new tasks, abort all owned work, and wait for every task to drop.
    pub async fn shutdown(&self) {
        let handles = {
            let mut state = self.state.lock().unwrap();
            state.closed = true;
            std::mem::take(&mut state.handles)
        };
        for handle in &handles {
            handle.abort();
        }
        for handle in handles {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct DropMarker(Arc<AtomicBool>);

    impl Drop for DropMarker {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn shutdown_waits_for_task_state_to_drop() {
        let scope = TaskScope::new();
        let dropped = Arc::new(AtomicBool::new(false));
        let marker = DropMarker(Arc::clone(&dropped));
        assert!(scope.spawn(async move {
            let _marker = marker;
            std::future::pending::<()>().await;
        }));

        scope.shutdown().await;

        assert!(dropped.load(Ordering::SeqCst));
        assert!(!scope.spawn(async {}));
    }
}
