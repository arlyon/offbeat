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

pub struct GroupCreateResultDto {
    pub group_id: String,
    pub invite_payload: String,
}

pub struct GroupJoinResultDto {
    pub group_id: String,
}

pub struct GroupMemberDto {
    pub user_id: String,
    pub display_name: String,
    pub status: String,
    pub stage_id: Option<String>,
    pub custom_location: Option<String>,
}

pub struct GroupPinDto {
    pub id: String,
    pub label: String,
    pub location: String,
    pub pinned_by: String,
}

pub struct GroupStateDto {
    pub name: String,
    pub members: Vec<GroupMemberDto>,
    pub pins: Vec<GroupPinDto>,
}

pub struct IdentityDto {
    pub user_id: String,
    pub display_name: Option<String>,
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

    /// Send a festival chat message (plaintext) and broadcast it via gossip if
    /// networking is active.  Returns the persisted message.
    ///
    /// WS relay delivery should be performed separately by the caller via
    /// `connect_relay`.
    pub async fn send_festival_chat(
        &self,
        festival_id: String,
        stage_id: Option<String>,
        text: String,
    ) -> anyhow::Result<ChatMessageDto> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let display_name = auth::get_display_name(&self.inner.db)?.unwrap_or_else(|| user_id.clone());

        let (msg, topic_id) = self.inner.chat_manager.send_festival_chat(
            &festival_id,
            stage_id.as_deref(),
            &user_id,
            &display_name,
            &text,
        )?;

        // Broadcast via gossip if available.
        if let Some(gm) = &self.inner.gossip_manager {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};
            let wire = encode_gossip_message_pub(&GossipMessage::Chat(msg.clone()))?;
            let bytes = serde_json::to_vec(&wire)?;
            // Best-effort broadcast — ignore errors (e.g. not subscribed yet).
            let _ = gm.lock().await.publish(topic_id, bytes).await;
        }

        Ok(ChatMessageDto {
            id: msg.id,
            user_id: msg.user_id,
            display_name: msg.display_name,
            text: msg.text,
            topic: msg.topic,
            stage_id: msg.stage_id,
            timestamp: msg.timestamp,
        })
    }

    /// Send an encrypted group chat message and broadcast it via gossip if
    /// networking is active.  Returns the persisted message.
    pub async fn send_group_chat(
        &self,
        group_id: String,
        text: String,
    ) -> anyhow::Result<ChatMessageDto> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let display_name = auth::get_display_name(&self.inner.db)?.unwrap_or_else(|| user_id.clone());

        let (encrypted, topic_id) = self.inner.chat_manager.send_group_chat(
            &group_id,
            &user_id,
            &display_name,
            &text,
        )?;

        // Retrieve the stored message to return as DTO (most recent for topic).
        let stored = self
            .inner
            .db
            .get_chat_messages(&format!("group/{group_id}/chat"), 1, 0)?;
        let msg = stored
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("send_group_chat: message not found after save"))?;

        // Broadcast raw encrypted bytes via gossip if available.
        if let Some(gm) = &self.inner.gossip_manager {
            let _ = gm.lock().await.publish(topic_id, encrypted).await;
        }

        Ok(ChatMessageDto {
            id: msg.id,
            user_id: msg.user_id,
            display_name: msg.display_name,
            text: msg.text,
            topic: msg.topic,
            stage_id: msg.stage_id,
            timestamp: msg.timestamp,
        })
    }

    /// Return paginated chat history for a topic string.
    pub fn get_chat_history(
        &self,
        topic: String,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<ChatMessageDto>> {
        let msgs = self.inner.chat_manager.get_history(&topic, limit, offset)?;
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

    /// Subscribe to all chat topics for a festival (general + campsite + per-stage).
    pub async fn subscribe_chat_topics(
        &self,
        festival_id: String,
        stage_ids: Vec<String>,
    ) -> anyhow::Result<()> {
        let stage_refs: Vec<&str> = stage_ids.iter().map(String::as_str).collect();
        let chat_topics = self
            .inner
            .chat_manager
            .get_festival_chat_topics(&festival_id, &stage_refs);

        if let Some(gm) = &self.inner.gossip_manager {
            let mut gm_locked = gm.lock().await;
            for (_topic_str, topic_id) in &chat_topics {
                gm_locked.subscribe(*topic_id, vec![]).await?;
            }
        }

        Ok(())
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

    // -----------------------------------------------------------------------
    // Identity
    // -----------------------------------------------------------------------

    /// Return the local identity (user_id + optional display_name).
    pub fn get_identity(&self) -> anyhow::Result<IdentityDto> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let display_name = auth::get_display_name(&self.inner.db)?;
        Ok(IdentityDto { user_id, display_name })
    }

    /// Persist a display name for the local user.
    pub fn set_display_name(&self, name: String) -> anyhow::Result<()> {
        offbeat_core::auth::set_display_name(&self.inner.db, &name)
    }

    // -----------------------------------------------------------------------
    // Group lifecycle
    // -----------------------------------------------------------------------

    /// Create a new group and return its ID + shareable invite payload.
    pub async fn create_group(
        &self,
        festival_id: String,
        name: String,
        display_name: String,
    ) -> anyhow::Result<GroupCreateResultDto> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        let result = self
            .inner
            .group_manager
            .create_group(&festival_id, &name, &user_id, &display_name)
            .await?;

        Ok(GroupCreateResultDto {
            group_id: result.group_id,
            invite_payload: result.invite_payload,
        })
    }

    /// Join an existing group from an invite payload.
    pub async fn join_group(
        &self,
        invite_payload: String,
        display_name: String,
    ) -> anyhow::Result<GroupJoinResultDto> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        let result = self
            .inner
            .group_manager
            .join_group(&invite_payload, &user_id, &display_name)
            .await?;

        Ok(GroupJoinResultDto {
            group_id: result.group_id,
        })
    }

    /// Leave a group.
    pub async fn leave_group(&self, group_id: String) -> anyhow::Result<()> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        self.inner
            .group_manager
            .leave_group(&group_id, &user_id)
            .await?;
        Ok(())
    }

    /// Record the current location and optionally publish via gossip.
    pub async fn check_in(
        &self,
        group_id: String,
        stage_id: Option<String>,
        custom_location: Option<String>,
    ) -> anyhow::Result<()> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        let encrypted = self
            .inner
            .group_manager
            .check_in(
                &group_id,
                &user_id,
                stage_id.as_deref(),
                custom_location.as_deref(),
            )
            .await?;

        if let Some(gm) = &self.inner.gossip_manager {
            use offbeat_core::topics;
            if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
                let topic = topics::group_topic(&group_key, "state");
                gm.lock().await.publish(topic, encrypted).await?;
            }
        }
        Ok(())
    }

    /// Update the shared stars for the current user in a group.
    pub async fn update_shared_stars(
        &self,
        group_id: String,
        set_ids: Vec<String>,
    ) -> anyhow::Result<()> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        let encrypted = self
            .inner
            .group_manager
            .update_stars(&group_id, &user_id, set_ids)
            .await?;

        if let Some(gm) = &self.inner.gossip_manager {
            use offbeat_core::topics;
            if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
                let topic = topics::group_topic(&group_key, "state");
                gm.lock().await.publish(topic, encrypted).await?;
            }
        }
        Ok(())
    }

    /// Add a map pin to a group and optionally publish via gossip.
    pub async fn add_pin(
        &self,
        group_id: String,
        label: String,
        location: String,
    ) -> anyhow::Result<()> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        let pin_id = uuid::Uuid::new_v4().to_string();
        let encrypted = self
            .inner
            .group_manager
            .add_pin(&group_id, &pin_id, &label, &location, &user_id)
            .await?;

        if let Some(gm) = &self.inner.gossip_manager {
            use offbeat_core::topics;
            if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
                let topic = topics::group_topic(&group_key, "state");
                gm.lock().await.publish(topic, encrypted).await?;
            }
        }
        Ok(())
    }

    /// Read the current state of a group from the local Yrs doc.
    pub async fn get_group_state(&self, group_id: String) -> anyhow::Result<GroupStateDto> {
        let state = self.inner.group_manager.get_group_state(&group_id).await?;

        Ok(GroupStateDto {
            name: state.name,
            members: state
                .members
                .into_iter()
                .map(|m| GroupMemberDto {
                    user_id: m.user_id,
                    display_name: m.display_name,
                    status: m.status,
                    stage_id: m.stage_id,
                    custom_location: m.custom_location,
                })
                .collect(),
            pins: state
                .pins
                .into_iter()
                .map(|p| GroupPinDto {
                    id: p.id,
                    label: p.label,
                    location: p.location,
                    pinned_by: p.pinned_by,
                })
                .collect(),
        })
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
