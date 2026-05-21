pub mod crypto;
pub mod db;
pub mod doc_manager;
pub mod gossip_manager;
pub mod signing;
pub mod topics;
pub mod transport;
pub mod types;

use std::sync::Arc;
use tokio::sync::Mutex;

use db::Database;
use doc_manager::DocManager;

/// Top-level node that ties together the database and document manager.
/// The iroh endpoint and gossip handle will be added in a later phase.
pub struct OffbeatNode {
    pub doc_manager: Arc<Mutex<DocManager>>,
    pub db: Arc<Database>,
}

impl OffbeatNode {
    /// Open (or create) the database at `db_path` and initialise the node.
    pub fn new(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let db = Arc::new(Database::new(db_path)?);
        let doc_manager = Arc::new(Mutex::new(DocManager::new(db.clone())));
        Ok(Self { doc_manager, db })
    }

    /// Create an in-memory node (useful for tests).
    pub fn new_in_memory() -> anyhow::Result<Self> {
        let db = Arc::new(Database::new_in_memory()?);
        let doc_manager = Arc::new(Mutex::new(DocManager::new(db.clone())));
        Ok(Self { doc_manager, db })
    }
}
