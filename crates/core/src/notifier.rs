//! ResourceNotifier — reactive notification system for resource updates.
//!
//! This module provides watch channels that emit when resources are updated,
//! allowing the UI to reactively subscribe to changes.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::watch;

// ---------------------------------------------------------------------------
// DTOs for notification payloads
// ---------------------------------------------------------------------------

/// Sync status for a single resource.
#[derive(Debug, Clone, Default)]
pub struct ResourceSyncStatus {
    /// Resource ID.
    pub id: String,
    /// Whether the resource is currently syncing.
    pub syncing: bool,
    /// Last sync timestamp (ISO 8601).
    pub last_synced: Option<String>,
    /// Error message if last sync failed.
    pub error: Option<String>,
}

/// Overall sync status.
#[derive(Debug, Clone, Default)]
pub struct SyncStatus {
    /// Whether any sync is in progress.
    pub syncing: bool,
    /// Per-resource status.
    pub resources: Vec<ResourceSyncStatus>,
    /// Number of pending operations.
    pub pending_ops: u32,
}

// ---------------------------------------------------------------------------
// ResourceNotifier
// ---------------------------------------------------------------------------

/// Notification hub for resource updates.
///
/// Provides watch channels that emit when documents or chat are updated.
/// UI components subscribe to these channels to react to changes.
pub struct ResourceNotifier {
    /// Per-doc watch senders (doc_id → sender).
    /// When a doc is updated, we send () to trigger watchers.
    doc_senders: RwLock<HashMap<String, watch::Sender<()>>>,

    /// Per-topic chat watch senders (topic → sender).
    chat_senders: RwLock<HashMap<String, watch::Sender<()>>>,

    /// Global sync status sender.
    sync_status_tx: watch::Sender<SyncStatus>,

    /// Global sync status receiver (for cloning).
    sync_status_rx: watch::Receiver<SyncStatus>,
}

impl ResourceNotifier {
    /// Create a new ResourceNotifier.
    pub fn new() -> Self {
        let (sync_status_tx, sync_status_rx) = watch::channel(SyncStatus::default());
        Self {
            doc_senders: RwLock::new(HashMap::new()),
            chat_senders: RwLock::new(HashMap::new()),
            sync_status_tx,
            sync_status_rx,
        }
    }

    // -----------------------------------------------------------------------
    // Document notifications
    // -----------------------------------------------------------------------

    /// Get or create a watch receiver for a document.
    ///
    /// The receiver will emit () each time the document is updated.
    pub fn watch_doc(&self, doc_id: &str) -> watch::Receiver<()> {
        let senders = self.doc_senders.read();
        if let Some(sender) = senders.get(doc_id) {
            return sender.subscribe();
        }
        drop(senders);

        // Need to create a new sender
        let mut senders = self.doc_senders.write();
        // Double-check after acquiring write lock
        if let Some(sender) = senders.get(doc_id) {
            return sender.subscribe();
        }

        let (tx, rx) = watch::channel(());
        senders.insert(doc_id.to_string(), tx);
        rx
    }

    /// Notify that a document was updated.
    pub fn notify_doc(&self, doc_id: &str) {
        let senders = self.doc_senders.read();
        if let Some(sender) = senders.get(doc_id) {
            let _ = sender.send(());
        }
    }

    // -----------------------------------------------------------------------
    // Chat notifications
    // -----------------------------------------------------------------------

    /// Get or create a watch receiver for chat messages on a topic.
    ///
    /// The receiver will emit () each time new messages arrive.
    pub fn watch_chat(&self, topic: &str) -> watch::Receiver<()> {
        let senders = self.chat_senders.read();
        if let Some(sender) = senders.get(topic) {
            return sender.subscribe();
        }
        drop(senders);

        // Need to create a new sender
        let mut senders = self.chat_senders.write();
        // Double-check after acquiring write lock
        if let Some(sender) = senders.get(topic) {
            return sender.subscribe();
        }

        let (tx, rx) = watch::channel(());
        senders.insert(topic.to_string(), tx);
        rx
    }

    /// Notify that new chat messages arrived on a topic.
    pub fn notify_chat(&self, topic: &str) {
        let senders = self.chat_senders.read();
        if let Some(sender) = senders.get(topic) {
            let _ = sender.send(());
        }
    }

    // -----------------------------------------------------------------------
    // Sync status notifications
    // -----------------------------------------------------------------------

    /// Get a receiver for sync status updates.
    pub fn watch_sync_status(&self) -> watch::Receiver<SyncStatus> {
        self.sync_status_rx.clone()
    }

    /// Update the sync status.
    pub fn notify_sync_status(&self, status: SyncStatus) {
        let _ = self.sync_status_tx.send(status);
    }

    /// Mark sync as started.
    pub fn sync_started(&self) {
        let mut status = self.sync_status_rx.borrow().clone();
        status.syncing = true;
        let _ = self.sync_status_tx.send(status);
    }

    /// Mark sync as completed.
    pub fn sync_completed(&self) {
        let mut status = self.sync_status_rx.borrow().clone();
        status.syncing = false;
        let _ = self.sync_status_tx.send(status);
    }

    // -----------------------------------------------------------------------
    // Cleanup
    // -----------------------------------------------------------------------

    /// Remove watch channels for a document (when unsubscribing).
    pub fn unwatch_doc(&self, doc_id: &str) {
        self.doc_senders.write().remove(doc_id);
    }

    /// Remove watch channels for a chat topic.
    pub fn unwatch_chat(&self, topic: &str) {
        self.chat_senders.write().remove(topic);
    }
}

impl Default for ResourceNotifier {
    fn default() -> Self {
        Self::new()
    }
}

// Make it easy to wrap in Arc
impl ResourceNotifier {
    /// Create a new Arc-wrapped ResourceNotifier.
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_doc_creates_channel() {
        let notifier = ResourceNotifier::new();
        let _rx = notifier.watch_doc("test-doc");

        // Should be able to get the same channel again
        let _rx2 = notifier.watch_doc("test-doc");
    }

    #[tokio::test]
    async fn test_notify_doc_triggers_watcher() {
        let notifier = ResourceNotifier::new();
        let mut rx = notifier.watch_doc("test-doc");

        // Mark current value as seen
        rx.mark_changed();

        // Notify
        notifier.notify_doc("test-doc");

        // Should have changed
        assert!(rx.has_changed().unwrap());
    }

    #[test]
    fn test_watch_chat_creates_channel() {
        let notifier = ResourceNotifier::new();
        let _rx = notifier.watch_chat("festival/test/chat");

        // Should be able to get the same channel again
        let _rx2 = notifier.watch_chat("festival/test/chat");
    }

    #[tokio::test]
    async fn test_notify_chat_triggers_watcher() {
        let notifier = ResourceNotifier::new();
        let mut rx = notifier.watch_chat("festival/test/chat");

        // Mark current value as seen
        rx.mark_changed();

        // Notify
        notifier.notify_chat("festival/test/chat");

        // Should have changed
        assert!(rx.has_changed().unwrap());
    }

    #[tokio::test]
    async fn test_sync_status_updates() {
        let notifier = ResourceNotifier::new();
        let mut rx = notifier.watch_sync_status();

        // Mark current value as seen
        rx.mark_changed();

        // Update status
        notifier.sync_started();

        // Should have changed
        assert!(rx.has_changed().unwrap());
        assert!(rx.borrow().syncing);

        rx.mark_changed();

        // Complete sync
        notifier.sync_completed();

        assert!(rx.has_changed().unwrap());
        assert!(!rx.borrow().syncing);
    }

    #[test]
    fn test_unwatch_doc_removes_channel() {
        let notifier = ResourceNotifier::new();
        let _rx = notifier.watch_doc("test-doc");

        notifier.unwatch_doc("test-doc");

        // Creating a new watcher should create a fresh channel
        let _rx2 = notifier.watch_doc("test-doc");
    }
}
