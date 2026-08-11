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

    /// Spawn on the current Tokio runtime unless shutdown has begun.
    ///
    /// Returns `false` instead of panicking when called outside a runtime.
    pub fn spawn<F>(&self, future: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return false;
        };
        self.spawn_on(&handle, future)
    }

    /// Spawn on an explicit Tokio runtime unless shutdown has begun.
    pub fn spawn_on<F>(&self, handle: &tokio::runtime::Handle, future: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return false;
        }
        state.handles.retain(|handle| !handle.is_finished());
        state.handles.push(handle.spawn(future));
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

    #[test]
    fn explicit_runtime_spawn_works_after_safe_outside_runtime_failure() {
        let scope = TaskScope::new();
        assert!(!scope.spawn(async {}));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let task_ran = Arc::clone(&ran);
        assert!(scope.spawn_on(runtime.handle(), async move {
            task_ran.store(true, Ordering::SeqCst);
        }));
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });

        assert!(ran.load(Ordering::SeqCst));
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
