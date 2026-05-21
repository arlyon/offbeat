use offbeat_core::OffbeatNode;

/// Initialize the Flutter Rust Bridge utilities. Must be called before any other bridge function.
#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

pub struct GroupInfo {
    pub id: String,
    pub name: String,
}

pub struct ChatMessageDto {
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    pub text: String,
    pub topic: String,
    pub stage_id: Option<String>,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Opaque node handle
// ---------------------------------------------------------------------------

/// Opaque handle to the running Offbeat node, used by all Dart callers.
#[flutter_rust_bridge::frb(opaque)]
pub struct AppNode {
    inner: OffbeatNode,
}

impl AppNode {
    /// Open (or create) the node database at `db_path`.
    pub fn create(db_path: String) -> anyhow::Result<AppNode> {
        let path = std::path::Path::new(&db_path);
        let inner = OffbeatNode::new(path)?;
        Ok(AppNode { inner })
    }

    /// Create an in-memory node (useful for testing).
    pub fn create_in_memory() -> anyhow::Result<AppNode> {
        let inner = OffbeatNode::new_in_memory()?;
        Ok(AppNode { inner })
    }

    /// Return the set IDs that are starred for the given festival.
    pub fn get_stars(&self, festival_id: String) -> anyhow::Result<Vec<String>> {
        self.inner.db.get_stars(&festival_id)
    }

    /// Toggle a star on a set. Returns the new starred state (`true` = now starred).
    pub fn toggle_star(&self, festival_id: String, set_id: String) -> anyhow::Result<bool> {
        self.inner.db.toggle_star(&festival_id, &set_id)
    }

    /// Persist a group record for a festival.
    pub fn save_group(
        &self,
        id: String,
        festival_id: String,
        name: String,
        key: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.inner.db.save_group(&id, &festival_id, &name, &key)
    }

    /// Return all groups for the given festival.
    pub fn get_groups(&self, festival_id: String) -> anyhow::Result<Vec<GroupInfo>> {
        let rows = self.inner.db.load_groups(&festival_id)?;
        Ok(rows
            .into_iter()
            .map(|(id, name, _key)| GroupInfo { id, name })
            .collect())
    }

    /// Delete a group by ID.
    pub fn delete_group(&self, id: String) -> anyhow::Result<()> {
        self.inner.db.delete_group(&id)
    }

    /// Fetch chat messages for a topic with pagination.
    pub fn get_chat_messages(
        &self,
        topic: String,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<ChatMessageDto>> {
        let msgs = self.inner.db.get_chat_messages(&topic, limit, offset)?;
        Ok(msgs
            .into_iter()
            .map(|m| ChatMessageDto {
                id: m.id,
                user_id: m.user_id,
                display_name: m.display_name,
                text: m.text,
                topic: m.topic,
                stage_id: m.stage_id,
                timestamp: m.timestamp,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Crypto utilities
// ---------------------------------------------------------------------------

/// Generate a fresh random 32-byte group key.
pub fn generate_group_key() -> Vec<u8> {
    offbeat_core::crypto::generate_group_key().to_vec()
}

/// Derive a stable group ID string from a 32-byte group key.
pub fn group_id_from_key(key: Vec<u8>) -> anyhow::Result<String> {
    let arr: [u8; 32] = key
        .try_into()
        .map_err(|_| anyhow::anyhow!("key must be exactly 32 bytes"))?;
    Ok(offbeat_core::crypto::group_id_from_key(&arr))
}
