use crate::frb_generated::StreamSink;
use offbeat_core::OffbeatNode;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Global tokio runtime for spawning watch tasks.
/// FRB's async executor isn't a full tokio runtime, so we need our own.
static RUNTIME: Lazy<Arc<Runtime>> = Lazy::new(|| {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create tokio runtime"),
    )
});

/// Initialize the Flutter Rust Bridge utilities. Must be called before any other bridge function.
#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
    // Force lazy initialization of the runtime
    let _ = &*RUNTIME;
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

pub struct AuthStateDto {
    pub state: String,
    pub expires_at: Option<String>,
}

pub struct AttestationDto {
    pub message: String,
    pub signature: String,
    pub issuer: String,
}

pub struct LineupStageDto {
    pub id: String,
    pub name: String,
    pub short: String,
    pub color: String,
    pub order: i32,
}

pub struct LineupDayDto {
    pub id: String,
    pub label: String,
    pub num: i32,
    pub month: String,
}

pub struct LineupSetDto {
    pub id: String,
    pub day: String,
    pub stage: String,
    pub artist: String,
    pub start_min: i32,
    pub duration_min: i32,
    pub genre: String,
    pub cancelled: bool,
}

pub struct LineupDto {
    pub stages: Vec<LineupStageDto>,
    pub days: Vec<LineupDayDto>,
    pub sets: Vec<LineupSetDto>,
}

/// Per-resource sync status.
pub struct ResourceSyncStatusDto {
    pub id: String,
    pub syncing: bool,
    pub last_synced: Option<String>,
    pub error: Option<String>,
}

/// Overall sync status for the node.
pub struct SyncStatusDto {
    pub syncing: bool,
    pub resources: Vec<ResourceSyncStatusDto>,
    pub pending_ops: u32,
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

    /// Read the lineup from the local Yrs doc for a festival.
    ///
    /// The Yrs doc at `festival/{id}/state` has separate root-map keys:
    /// `"stages"`, `"days"`, `"sets"` — each a JSON array string that
    /// arrives via signed gossip updates and merges independently.
    ///
    /// Returns `None` if no lineup data has synced yet.
    pub async fn get_lineup(&self, festival_id: String) -> Option<LineupDto> {
        let doc_id = format!("festival/{festival_id}/state");
        let mut dm = self.inner.doc_manager.lock().await;

        let stages = parse_json_array(dm.read_map_value(&doc_id, "stages"), |s| {
            Some(LineupStageDto {
                id: s.get("id")?.as_str()?.to_string(),
                name: s.get("name")?.as_str()?.to_string(),
                short: s.get("short")?.as_str()?.to_string(),
                color: s.get("color")?.as_str()?.to_string(),
                order: s.get("order")?.as_i64()? as i32,
            })
        });

        let days = parse_json_array(dm.read_map_value(&doc_id, "days"), |d| {
            Some(LineupDayDto {
                id: d.get("id")?.as_str()?.to_string(),
                label: d.get("label")?.as_str()?.to_string(),
                num: d.get("num")?.as_i64()? as i32,
                month: d.get("month")?.as_str()?.to_string(),
            })
        });

        let sets = parse_json_array(dm.read_map_value(&doc_id, "sets"), |s| {
            Some(LineupSetDto {
                id: s.get("id")?.as_str()?.to_string(),
                day: s.get("day")?.as_str()?.to_string(),
                stage: s.get("stage")?.as_str()?.to_string(),
                artist: s.get("artist")?.as_str()?.to_string(),
                start_min: s.get("startMin")?.as_i64()? as i32,
                duration_min: s.get("durationMin")?.as_i64()? as i32,
                genre: s.get("genre")?.as_str()?.to_string(),
                cancelled: s.get("cancelled")?.as_bool().unwrap_or(false),
            })
        });

        // Return None only if we have no data at all
        if stages.is_empty() && days.is_empty() && sets.is_empty() {
            return None;
        }

        Some(LineupDto { stages, days, sets })
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
    /// networking is active. Returns the persisted message.
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

        {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};
            let wire = encode_gossip_message_pub(&GossipMessage::Chat(msg.clone()))?;

            if let Some(gm) = &self.inner.gossip_manager {
                let bytes = serde_json::to_vec(&wire)?;
                let _ = gm.lock().await.broadcast(topic_id, bytes).await;
            }

            if let Some(ws) = &self.inner.ws_relay {
                let topic_str = format!(
                    "festival/{}/chat/{}",
                    festival_id,
                    stage_id.as_deref().unwrap_or("general")
                );
                let _ = ws.send_gossip(&topic_str, &wire).await;
            }
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
    /// networking is active. Returns the persisted message.
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

        let stored = self
            .inner
            .db
            .get_chat_messages(&format!("group/{group_id}/chat"), 1, 0)?;
        let msg = stored
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("send_group_chat: message not found after save"))?;

        {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};

            let group_key = self
                .inner
                .db
                .load_group_key(&group_id)?
                .ok_or_else(|| anyhow::anyhow!("group not found after send"))?;

            let wire = encode_gossip_message_pub(&GossipMessage::EncryptedChat {
                group_key,
                encrypted: encrypted.clone(),
            })?;

            if let Some(gm) = &self.inner.gossip_manager {
                let bytes = serde_json::to_vec(&wire)?;
                let _ = gm.lock().await.broadcast(topic_id, bytes).await;
            }

            if let Some(ws) = &self.inner.ws_relay {
                let topic_str = format!("group/{group_id}/chat");
                let _ = ws.send_gossip(&topic_str, &wire).await;
            }
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
    /// `festival_id` is used to look up the cached Ed25519 public key for
    /// verifying signed updates. Call `set_festival_public_key` first.
    pub async fn connect_relay(&mut self, url: String, festival_id: String) -> anyhow::Result<()> {
        use offbeat_core::{auth, ws_relay};
        use std::sync::Arc;

        let doc_manager = Arc::clone(&self.inner.doc_manager);
        let db = Arc::clone(&self.inner.db);
        let festival_pk = self.inner.festival_public_keys.get(&festival_id).copied()
            .ok_or_else(|| anyhow::anyhow!(
                "no public key for festival {festival_id} — call set_festival_public_key first"
            ))?;

        let (sink, receive_loop) = ws_relay::connect(
            &url,
            doc_manager,
            db,
            festival_pk,
        )
        .await?;

        if let Ok(Some(attestation)) = auth::load_attestation(&self.inner.db)
            && let Ok(signing_key) = auth::generate_or_load_identity(&self.inner.db)
        {
            let pubkey_hex = auth::get_public_key_hex(&signing_key);
            if let Err(e) = sink.authenticate(&pubkey_hex, &attestation, &signing_key).await {
                tracing::warn!("ws relay auth failed: {e}");
            }
        }

        self.inner.ws_relay = Some(Arc::new(sink));

        RUNTIME.spawn(async move {
            if let Err(e) = receive_loop.await {
                tracing::warn!("ws relay receive loop exited: {e}");
            }
        });

        Ok(())
    }

    /// Subscribe to the gossip topic for a festival and perform a state vector
    /// exchange with the DO so we only receive updates we don't already have.
    pub async fn subscribe_festival(&mut self, festival_id: String) -> anyhow::Result<()> {
        let topic_str = format!("festival/{festival_id}/state");
        let doc_id = topic_str.clone();

        if let Some(ws) = &self.inner.ws_relay {
            ws.subscribe(vec![topic_str]).await?;

            // Send our state vector so the DO can compute a targeted diff
            let sv = {
                let mut dm = self.inner.doc_manager.lock().await;
                dm.get_or_create(&doc_id);
                dm.get_state_vector(&doc_id)?
            };
            ws.sv_exchange(&doc_id, &sv).await?;
        }

        Ok(())
    }

    /// Cache a festival's Ed25519 public key (hex-encoded, 64 chars).
    pub fn set_festival_public_key(
        &mut self,
        festival_id: String,
        hex_key: String,
    ) -> anyhow::Result<()> {
        let bytes: Vec<u8> = (0..hex_key.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_key[i..i + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("invalid hex key: {e}"))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
        self.inner.festival_public_keys.insert(festival_id, key);
        Ok(())
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
            writer_seq: 0,
        };

        let parts: Vec<&str> = topic.splitn(3, '/').collect();
        let topic_id = if parts.len() == 3 && parts[0] == "festival" {
            offbeat_core::topics::festival_topic(parts[1], parts[2])
        } else {
            anyhow::bail!("unsupported topic format: {topic}");
        };

        let wire = encode_gossip_message_pub(&GossipMessage::Chat(chat))?;
        let bytes = serde_json::to_vec(&wire)?;

        if let Some(gm) = &self.inner.gossip_manager {
            let _ = gm.lock().await.broadcast(topic_id, bytes).await;
        }

        if let Some(ws) = &self.inner.ws_relay {
            let _ = ws.send_gossip(&topic, &wire).await;
        }

        Ok(())
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

    /// Derive the Ed25519 identity from a WebAuthn PRF output (32 bytes).
    pub fn derive_identity_from_prf(&self, prf_output: Vec<u8>) -> anyhow::Result<String> {
        let arr: [u8; 32] = prf_output
            .try_into()
            .map_err(|_| anyhow::anyhow!("PRF output must be exactly 32 bytes"))?;
        let key = offbeat_core::auth::derive_identity_from_prf(&self.inner.db, &arr)?;
        Ok(offbeat_core::auth::get_public_key_hex(&key))
    }

    /// Get the hex-encoded Ed25519 public key of the local identity.
    pub fn get_public_key_hex(&self) -> anyhow::Result<String> {
        let key = offbeat_core::auth::generate_or_load_identity(&self.inner.db)?;
        Ok(offbeat_core::auth::get_public_key_hex(&key))
    }

    /// Get the current authentication state.
    pub fn get_auth_state(&self) -> anyhow::Result<AuthStateDto> {
        use offbeat_core::auth::{self, AuthState};
        let state = auth::attestation_state(&self.inner.db)?;
        let (state_str, expires_at) = match &state {
            AuthState::Unregistered => ("unregistered".to_string(), None),
            AuthState::Valid => ("valid".to_string(), None),
            AuthState::Expiring(days) => ("expiring".to_string(), Some(format!("{days} days"))),
            AuthState::Expired => ("expired".to_string(), None),
        };
        Ok(AuthStateDto {
            state: state_str,
            expires_at,
        })
    }

    /// Store an attestation received from the MainDO.
    pub fn store_attestation(
        &self,
        message: String,
        signature: String,
        issuer: String,
    ) -> anyhow::Result<()> {
        let att = offbeat_core::auth::Attestation {
            message,
            signature,
            issuer,
        };
        offbeat_core::auth::store_attestation(&self.inner.db, &att)
    }

    /// Load the stored attestation, if any.
    pub fn get_attestation(&self) -> anyhow::Result<Option<AttestationDto>> {
        let att = offbeat_core::auth::load_attestation(&self.inner.db)?;
        Ok(att.map(|a| AttestationDto {
            message: a.message,
            signature: a.signature,
            issuer: a.issuer,
        }))
    }

    /// Sign an arbitrary message with the local Ed25519 identity key.
    pub fn sign_message(&self, message: String) -> anyhow::Result<String> {
        let key = offbeat_core::auth::generate_or_load_identity(&self.inner.db)?;
        let sig = offbeat_core::signing::sign(&key, message.as_bytes());
        Ok(sig.iter().map(|b| format!("{b:02x}")).collect())
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

        if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};
            let wire = encode_gossip_message_pub(&GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}"),
                encrypted: encrypted.clone(),
                group_key,
            })?;

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let bytes = serde_json::to_vec(&wire)?;
                let _ = gm.lock().await.broadcast(topic, bytes).await;
            }
            if let Some(ws) = &self.inner.ws_relay {
                let topic_str = format!("group/{group_id}/state");
                let _ = ws.send_gossip(&topic_str, &wire).await;
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

        if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};
            let wire = encode_gossip_message_pub(&GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}"),
                encrypted: encrypted.clone(),
                group_key,
            })?;

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let bytes = serde_json::to_vec(&wire)?;
                let _ = gm.lock().await.broadcast(topic, bytes).await;
            }
            if let Some(ws) = &self.inner.ws_relay {
                let topic_str = format!("group/{group_id}/state");
                let _ = ws.send_gossip(&topic_str, &wire).await;
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

        if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message_pub};
            let wire = encode_gossip_message_pub(&GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}"),
                encrypted: encrypted.clone(),
                group_key,
            })?;

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let bytes = serde_json::to_vec(&wire)?;
                let _ = gm.lock().await.broadcast(topic, bytes).await;
            }
            if let Some(ws) = &self.inner.ws_relay {
                let topic_str = format!("group/{group_id}/state");
                let _ = ws.send_gossip(&topic_str, &wire).await;
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

    // -----------------------------------------------------------------------
    // Reactive stream methods
    // -----------------------------------------------------------------------

    /// Watch festival lineup — emits current state, then updates on changes.
    ///
    /// The stream emits the current lineup immediately, then re-emits whenever
    /// the lineup document is updated (via sync or local changes).
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_lineup(
        &self,
        festival_id: String,
        sink: StreamSink<Option<LineupDto>>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let doc_id = format!("festival/{festival_id}/state");
        let doc_manager = Arc::clone(&self.inner.doc_manager);
        let notifier = Arc::clone(&self.inner.notifier);

        // Subscribe to doc updates
        let mut rx = notifier.watch_doc(&doc_id);

        // Spawn task to emit on changes
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);
        let doc_manager_clone = Arc::clone(&doc_manager);
        let doc_id_clone = doc_id.clone();

        RUNTIME.spawn(async move {
            // Emit initial state
            {
                let mut dm = doc_manager_clone.lock().await;
                let lineup = read_lineup_from_doc(&mut dm, &doc_id_clone);
                let sink = sink_clone.lock().await;
                let _ = sink.add(lineup);
            }

            // Watch for changes
            loop {
                if rx.changed().await.is_err() {
                    break; // Channel closed
                }
                let mut dm = doc_manager_clone.lock().await;
                let lineup = read_lineup_from_doc(&mut dm, &doc_id_clone);
                let sink = sink_clone.lock().await;
                if sink.add(lineup).is_err() {
                    break; // Sink closed
                }
            }
        });

        Ok(())
    }

    /// Watch group state — emits current state, then updates on changes.
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_group_state(
        &self,
        group_id: String,
        sink: StreamSink<GroupStateDto>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let doc_id = format!("group/{group_id}");
        let group_manager = Arc::clone(&self.inner.group_manager);
        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_doc(&doc_id);
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);
        let group_id_clone = group_id.clone();

        RUNTIME.spawn(async move {
            // Emit initial state
            if let Ok(state) = group_manager.get_group_state(&group_id_clone).await {
                let dto = GroupStateDto {
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
                };
                let sink = sink_clone.lock().await;
                let _ = sink.add(dto);
            }

            // Watch for changes
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                if let Ok(state) = group_manager.get_group_state(&group_id_clone).await {
                    let dto = GroupStateDto {
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
                    };
                    let sink = sink_clone.lock().await;
                    if sink.add(dto).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Watch chat messages for a topic — emits current messages, then updates.
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_chat(
        &self,
        topic: String,
        last_n: u32,
        sink: StreamSink<Vec<ChatMessageDto>>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let db = Arc::clone(&self.inner.db);
        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_chat(&topic);
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);
        let topic_clone = topic.clone();

        RUNTIME.spawn(async move {
            // Emit initial messages
            if let Ok(msgs) = db.get_chat_messages(&topic_clone, last_n, 0) {
                let dtos: Vec<ChatMessageDto> = msgs
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
                    .collect();
                let sink = sink_clone.lock().await;
                let _ = sink.add(dtos);
            }

            // Watch for changes
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                if let Ok(msgs) = db.get_chat_messages(&topic_clone, last_n, 0) {
                    let dtos: Vec<ChatMessageDto> = msgs
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
                        .collect();
                    let sink = sink_clone.lock().await;
                    if sink.add(dtos).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Watch sync status — emits current status, then updates on changes.
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_sync_status(
        &self,
        sink: StreamSink<SyncStatusDto>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_sync_status();
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);

        RUNTIME.spawn(async move {
            // Emit initial status
            {
                let status = rx.borrow().clone();
                let dto = convert_sync_status(&status);
                let sink = sink_clone.lock().await;
                let _ = sink.add(dto);
            }

            // Watch for changes
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let status = rx.borrow().clone();
                let dto = convert_sync_status(&status);
                let sink = sink_clone.lock().await;
                if sink.add(dto).is_err() {
                    break;
                }
            }
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a JSON array string from a Yrs map value, mapping each element with `f`.
fn parse_json_array<T>(
    raw: Option<String>,
    f: impl Fn(&serde_json::Value) -> Option<T>,
) -> Vec<T> {
    let Some(s) = raw else { return vec![] };
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&s) else {
        return vec![];
    };
    arr.iter().filter_map(f).collect()
}

/// Read lineup from a doc manager (used by watch_lineup).
fn read_lineup_from_doc(
    dm: &mut offbeat_core::doc_manager::DocManager,
    doc_id: &str,
) -> Option<LineupDto> {
    let stages = parse_json_array(dm.read_map_value(doc_id, "stages"), |s| {
        Some(LineupStageDto {
            id: s.get("id")?.as_str()?.to_string(),
            name: s.get("name")?.as_str()?.to_string(),
            short: s.get("short")?.as_str()?.to_string(),
            color: s.get("color")?.as_str()?.to_string(),
            order: s.get("order")?.as_i64()? as i32,
        })
    });

    let days = parse_json_array(dm.read_map_value(doc_id, "days"), |d| {
        Some(LineupDayDto {
            id: d.get("id")?.as_str()?.to_string(),
            label: d.get("label")?.as_str()?.to_string(),
            num: d.get("num")?.as_i64()? as i32,
            month: d.get("month")?.as_str()?.to_string(),
        })
    });

    let sets = parse_json_array(dm.read_map_value(doc_id, "sets"), |s| {
        Some(LineupSetDto {
            id: s.get("id")?.as_str()?.to_string(),
            day: s.get("day")?.as_str()?.to_string(),
            stage: s.get("stage")?.as_str()?.to_string(),
            artist: s.get("artist")?.as_str()?.to_string(),
            start_min: s.get("startMin")?.as_i64()? as i32,
            duration_min: s.get("durationMin")?.as_i64()? as i32,
            genre: s.get("genre")?.as_str()?.to_string(),
            cancelled: s.get("cancelled")?.as_bool().unwrap_or(false),
        })
    });

    if stages.is_empty() && days.is_empty() && sets.is_empty() {
        return None;
    }

    Some(LineupDto { stages, days, sets })
}

/// Convert SyncStatus from notifier to DTO.
fn convert_sync_status(status: &offbeat_core::notifier::SyncStatus) -> SyncStatusDto {
    SyncStatusDto {
        syncing: status.syncing,
        resources: status
            .resources
            .iter()
            .map(|r| ResourceSyncStatusDto {
                id: r.id.clone(),
                syncing: r.syncing,
                last_synced: r.last_synced.clone(),
                error: r.error.clone(),
            })
            .collect(),
        pending_ops: status.pending_ops,
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
