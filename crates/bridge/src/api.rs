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

    // -----------------------------------------------------------------------
    // Networking methods
    // -----------------------------------------------------------------------

    /// Connect this node to the Festival Durable Object relay at `url`.
    ///
    /// Spawns a background task that subscribes to topics and feeds incoming
    /// messages into the dispatch pipeline.  The connection runs until the
    /// node is dropped or the WebSocket is closed by the server.
    pub async fn connect_relay(&self, url: String) -> anyhow::Result<()> {
        use offbeat_core::ws_relay::WsRelay;
        use std::sync::Arc;

        let mut relay = WsRelay::connect(&url).await?;

        // Immediately subscribe to no topics — callers use `subscribe_festival`
        // afterwards to add topics.  This just establishes the connection.
        let doc_manager = Arc::clone(&self.inner.doc_manager);
        let db = Arc::clone(&self.inner.db);

        tokio::spawn(async move {
            if let Err(e) = relay.run_receive_loop(doc_manager, db, None).await {
                tracing::warn!("ws relay receive loop exited: {e}");
            }
        });

        Ok(())
    }

    /// Subscribe to the gossip topic for a festival, using the iroh-gossip
    /// layer (if networking was started).
    pub async fn subscribe_festival(
        &self,
        festival_id: String,
    ) -> anyhow::Result<()> {
        let topic_id = offbeat_core::topics::festival_topic(&festival_id, "state");

        let gm = self
            .inner
            .gossip_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("networking not started — call start_networking first"))?;

        gm.lock()
            .await
            .subscribe(topic_id, vec![])
            .await
    }

    /// Broadcast a chat message on the given gossip topic.
    pub async fn publish_chat(
        &self,
        topic: String,
        message: ChatMessageDto,
    ) -> anyhow::Result<()> {
        use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};
        use offbeat_core::types::ChatMessage;

        let chat = ChatMessage {
            id: message.id,
            user_id: message.user_id,
            display_name: message.display_name,
            text: message.text,
            topic: topic.clone(),
            stage_id: message.stage_id,
            timestamp: message.timestamp,
        };

        // Derive the topic id from the topic string.
        // Expected format: "festival/{id}/{channel}"
        let parts: Vec<&str> = topic.splitn(3, '/').collect();
        let topic_id = if parts.len() == 3 && parts[0] == "festival" {
            offbeat_core::topics::festival_topic(parts[1], parts[2])
        } else {
            anyhow::bail!("unsupported topic format: {topic}");
        };

        let wire = encode_gossip_message_pub(&GossipMessage::Chat(chat))?;
        let bytes = serde_json::to_vec(&wire)?;

        let gm = self
            .inner
            .gossip_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("networking not started"))?;

        gm.lock().await.publish(topic_id, bytes).await
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
