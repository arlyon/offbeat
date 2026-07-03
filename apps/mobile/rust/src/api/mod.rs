pub mod dto;

use base64::Engine as _;
use crate::frb_generated::StreamSink;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::runtime::Runtime;

// Re-export DTOs and utilities for convenience
pub use dto::*;

// Re-export types used in public function signatures for FRB
pub use offbeat_core::connection_manager::PeerEntry;
pub use offbeat_core::doc_manager::DocManager;
pub use offbeat_core::notifier::SyncStatus;
pub use offbeat_core::OffbeatNode;

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
    // Install rustls crypto provider before anything else — both ring and aws-lc-rs
    // are in the dep tree, so rustls can't auto-detect.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    // Set up logging BEFORE FRB's setup_default_user_utils (which installs
    // its own subscriber — ours must win).
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_filter(
                    android_logger::FilterBuilder::new()
                        .parse("offbeat_core=info,iroh_ble_transport=info,blew=warn,iroh=warn,iroh_gossip=info,info")
                        .build(),
                )
                .with_tag("offbeat"),
        );
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    tracing_subscriber::EnvFilter::new(
                        "offbeat_core=info,iroh_ble_transport=info,blew=warn,iroh=warn,iroh_gossip=info,info",
                    )
                }),
            )
            .try_init();
    }

    flutter_rust_bridge::setup_default_user_utils();

    // Force lazy initialization of the runtime
    let _ = &*RUNTIME;
}

/// Scan for Meshtastic radios advertising the official BLE service UUID.
#[cfg(target_os = "android")]
pub async fn meshtastic_debug_scan(scan_ms: u32) -> anyhow::Result<Vec<MeshtasticDebugDeviceDto>> {
    use blew::central::{Central, ScanFilter, ScanMode};
    use tokio::time::{Duration, sleep};
    use uuid::Uuid;

    let central: Central = Central::new().await?;
    let service_uuid = Uuid::parse_str(MESHTASTIC_SERVICE_UUID)?;
    central
        .start_scan(ScanFilter {
            services: vec![service_uuid],
            mode: ScanMode::LowLatency,
        })
        .await?;
    sleep(Duration::from_millis(scan_ms.max(500) as u64)).await;
    let devices = central.discovered_devices().await?;
    let _ = central.stop_scan().await;
    Ok(devices
        .into_iter()
        .filter(|device| device.services.contains(&service_uuid))
        .map(|device| MeshtasticDebugDeviceDto {
            device_id: device.id.to_string(),
            name: device.name,
            rssi: device.rssi,
            services: device.services.into_iter().map(|uuid| uuid.to_string()).collect(),
        })
        .collect())
}

#[cfg(not(target_os = "android"))]
#[allow(clippy::unused_async)]
pub async fn meshtastic_debug_scan(_scan_ms: u32) -> anyhow::Result<Vec<MeshtasticDebugDeviceDto>> {
    anyhow::bail!("Meshtastic debug scan is currently wired for Android builds only")
}

/// Connect to a Meshtastic radio, optionally send a synthetic Offbeat frame,
/// then listen for `FromNum` notifications and drain `FromRadio` protobufs.
#[cfg(target_os = "android")]
pub async fn meshtastic_debug_probe(
    device_id: String,
    body: Vec<u8>,
    listen_ms: u32,
    send: bool,
) -> anyhow::Result<MeshtasticDebugReportDto> {
    use blew::central::{Central, CentralEvent, WriteType};
    use blew::types::DeviceId;
    use offbeat_core::transport::meshtastic::{
        encode_to_radio_private_app, MeshtasticReassembly, OffbeatSyncFrame, DEFAULT_TTL,
    };
    use offbeat_core::transport::profile::SyncPayloadKind;
    use tokio::time::{Duration, Instant, timeout};
    use tokio_stream::StreamExt;
    use uuid::Uuid;

    let central: Central = Central::new().await?;
    let device = DeviceId::from(device_id.clone());
    let service_uuid = Uuid::parse_str(MESHTASTIC_SERVICE_UUID)?;
    let from_num_uuid = Uuid::parse_str(MESHTASTIC_FROM_NUM_CHAR_UUID)?;
    let from_radio_uuid = Uuid::parse_str(MESHTASTIC_FROM_RADIO_CHAR_UUID)?;
    let to_radio_uuid = Uuid::parse_str(MESHTASTIC_TO_RADIO_CHAR_UUID)?;

    let mut events_log = Vec::new();
    let mut sent_fragments = 0u32;
    let mut raw_from_radio_count = 0u32;
    let mut private_app_count = 0u32;
    let mut reassembly = MeshtasticReassembly::default();
    let mut received_frames = Vec::new();
    let mut events = central.events();

    central
        .start_scan(blew::central::ScanFilter {
            services: vec![service_uuid],
            mode: blew::central::ScanMode::LowLatency,
        })
        .await?;
    tokio::time::sleep(Duration::from_millis(750)).await;
    let _ = central.stop_scan().await;

    central.connect(&device).await?;
    events_log.push(format!("connected:{device_id}"));

    let services = central.discover_services(&device).await?;
    let service_summaries = services
        .iter()
        .map(|service| {
            let chars = service
                .characteristics
                .iter()
                .map(|char_| char_.uuid.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("{}[{chars}]", service.uuid)
        })
        .collect::<Vec<_>>();

    central
        .subscribe_characteristic(&device, from_num_uuid)
        .await?;
    events_log.push("subscribed:from_num".to_string());

    let mtu = central.mtu(&device).await;

    if send {
        let topic_tag = *b"OBDEBUG!";
        let message_id = first_eight_uuid_bytes(uuid::Uuid::new_v4());
        let body = if body.is_empty() {
            b"offbeat meshtastic debug probe".to_vec()
        } else {
            body
        };
        let frame = OffbeatSyncFrame::new(
            SyncPayloadKind::GroupUpdate,
            topic_tag,
            message_id,
            DEFAULT_TTL,
            body,
        )?;
        for private_payload in frame.encode_private_app_payloads()? {
            let to_radio = encode_to_radio_private_app(
                private_payload,
                SyncPayloadKind::GroupUpdate.priority(),
                DEFAULT_TTL,
                false,
            )?;
            central
                .write_characteristic(&device, to_radio_uuid, to_radio, WriteType::WithResponse)
                .await?;
            sent_fragments += 1;
        }
        events_log.push(format!("sent_fragments:{sent_fragments}"));
    }

    let deadline = Instant::now() + Duration::from_millis(listen_ms.max(1000) as u64);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Some(event) = timeout(remaining, events.next()).await? else {
            break;
        };
        match event {
            CentralEvent::CharacteristicNotification {
                device_id: event_device,
                char_uuid,
                ..
            } if event_device == device && char_uuid == from_num_uuid => {
                events_log.push("notify:from_num".to_string());
                drain_from_radio(
                    &central,
                    &device,
                    from_radio_uuid,
                    &mut reassembly,
                    &mut raw_from_radio_count,
                    &mut private_app_count,
                    &mut received_frames,
                )
                .await?;
            }
            CentralEvent::DeviceDisconnected { device_id, cause } if device_id == device => {
                events_log.push(format!("disconnected:{cause:?}"));
                break;
            }
            CentralEvent::DeviceConnected { device_id } if device_id == device => {
                events_log.push("event:connected".to_string());
            }
            _ => {}
        }
    }

    let _ = central.disconnect(&device).await;

    Ok(MeshtasticDebugReportDto {
        device_id,
        connected: true,
        mtu,
        services: service_summaries,
        sent_fragments,
        raw_from_radio_count,
        private_app_count,
        received_frames,
        events: events_log,
    })
}

#[cfg(not(target_os = "android"))]
#[allow(clippy::unused_async)]
pub async fn meshtastic_debug_probe(
    device_id: String,
    _body: Vec<u8>,
    _listen_ms: u32,
    _send: bool,
) -> anyhow::Result<MeshtasticDebugReportDto> {
    anyhow::bail!("Meshtastic debug probe is currently wired for Android builds only: {device_id}")
}

#[cfg(target_os = "android")]
async fn drain_from_radio(
    central: &blew::central::Central,
    device: &blew::types::DeviceId,
    from_radio_uuid: uuid::Uuid,
    reassembly: &mut offbeat_core::transport::meshtastic::MeshtasticReassembly,
    raw_count: &mut u32,
    private_app_count: &mut u32,
    frames: &mut Vec<MeshtasticDebugFrameDto>,
) -> anyhow::Result<()> {
    use offbeat_core::transport::meshtastic::{decode_from_radio_private_app, MeshtasticPacket};

    loop {
        let raw = central.read_characteristic(device, from_radio_uuid).await?;
        if raw.is_empty() {
            break;
        }
        *raw_count += 1;
        let Some(private_payload) = decode_from_radio_private_app(&raw)? else {
            continue;
        };
        *private_app_count += 1;
        let packet = MeshtasticPacket::decode(&private_payload)?;
        if let Some(frame) = reassembly.push(packet)? {
            frames.push(MeshtasticDebugFrameDto {
                kind: format!("{:?}", frame.kind),
                topic_tag_hex: hex::encode(frame.topic_tag),
                message_id_hex: hex::encode(frame.message_id),
                body_text: String::from_utf8(frame.body.clone()).ok(),
                body_hex: hex::encode(frame.body),
            });
        }
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn first_eight_uuid_bytes(uuid: uuid::Uuid) -> [u8; 8] {
    let mut out = [0u8; 8];
    out.copy_from_slice(&uuid.as_bytes()[..8]);
    out
}

#[cfg(target_os = "android")]
const MESHTASTIC_SERVICE_UUID: &str = "6ba1b218-15a8-461f-9fa8-5dcae273eafd";
#[cfg(target_os = "android")]
const MESHTASTIC_TO_RADIO_CHAR_UUID: &str = "f75c76d2-129e-4dad-a1dd-7866124401e7";
#[cfg(target_os = "android")]
const MESHTASTIC_FROM_NUM_CHAR_UUID: &str = "ed9da18c-a800-4f66-a670-aa7547e34453";
#[cfg(target_os = "android")]
const MESHTASTIC_FROM_RADIO_CHAR_UUID: &str = "2c55e69e-4993-11ed-b878-0242ac120002";

// ---------------------------------------------------------------------------
// Opaque node handle
// ---------------------------------------------------------------------------

/// Opaque handle to the running Offbeat node, used by all Dart callers.
#[flutter_rust_bridge::frb(opaque)]
pub struct AppNode {
    inner: OffbeatNode,
    /// Join handles for BLE connection background tasks.
    ble_task_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl AppNode {
    /// Open (or create) the node database at `db_path` with full networking
    /// (iroh endpoint, gossip, and BLE transport if available).
    pub async fn create(db_path: String) -> anyhow::Result<AppNode> {
        let path = std::path::PathBuf::from(db_path);
        let inner = RUNTIME
            .spawn(async move { OffbeatNode::new_with_networking(&path).await })
            .await??;
        Ok(AppNode {
            inner,
            ble_task_handles: Vec::new(),
        })
    }

    /// Create an in-memory node (useful for testing).
    pub fn create_in_memory() -> anyhow::Result<AppNode> {
        let _guard = RUNTIME.enter();
        let inner = OffbeatNode::new_in_memory()?;
        Ok(AppNode {
            inner,
            ble_task_handles: Vec::new(),
        })
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
    /// Returns `None` if no lineup data has synced yet.
    pub async fn get_lineup(&self, festival_id: String) -> Option<LineupDto> {
        let doc_id = format!("festival/{festival_id}/state");
        read_lineup_from_doc(&self.inner.doc_manager, &doc_id)
    }

    /// Read the weather forecast from the local Yrs doc for a festival.
    ///
    /// Returns `None` if no weather data has synced yet.
    pub async fn get_weather(&self, festival_id: String) -> Option<WeatherForecastDto> {
        let doc_id = format!("festival/{festival_id}/state");
        read_weather_from_doc(&self.inner.doc_manager, &doc_id)
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

    /// Reconstruct the invite payload URI for an existing group.
    ///
    /// Returns `offbeat://group/{festival_id}/{group_id}/{base64url(key)}`
    /// or `None` if the group is not found.
    pub fn get_invite_payload(
        &self,
        group_id: String,
        festival_id: String,
    ) -> anyhow::Result<Option<String>> {
        let key = self.inner.db.load_group_key(&group_id)?;
        Ok(key.map(|k| {
            let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(k);
            format!("offbeat://group/{festival_id}/{group_id}/{b64}")
        }))
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
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;
            let gossip_msg = GossipMessage::Chat(msg.clone());
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);

            if let Some(gm) = &self.inner.gossip_manager {
                let _ = gm.lock().await.broadcast(topic_id, bytes).await;
            }

            if let Some(ws) = { self.inner.ws_relay.read().clone() } {
                let topic_str = format!(
                    "festival/{}/chat/{}",
                    festival_id,
                    stage_id.as_deref().unwrap_or("general")
                );
                let _ = ws.send_gossip(&topic_str, &envelope).await;
            }
        }

        self.inner.notifier.record_sent(&msg.topic);

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
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;

            let group_key = self
                .inner
                .db
                .load_group_key(&group_id)?
                .ok_or_else(|| anyhow::anyhow!("group not found after send"))?;

            let gossip_msg = GossipMessage::EncryptedChat {
                group_key,
                encrypted: encrypted.clone(),
            };
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);

            if let Some(gm) = &self.inner.gossip_manager {
                let _ = gm.lock().await.broadcast(topic_id, bytes).await;
            }

            if let Some(ws) = { self.inner.ws_relay.read().clone() } {
                let topic_str = format!("group/{group_id}/chat");
                let _ = ws.send_gossip(&topic_str, &envelope).await;
            }
        }

        let resource_id = format!("group/{group_id}/chat");
        self.inner.notifier.record_sent(&resource_id);
        // Notify local chat watchers so the UI updates immediately
        self.inner.notifier.notify_chat(&resource_id);

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
            // Seed the gossip overlay from the durable peer directory so it can
            // form even when the relay is unreachable (offline cold-start).
            let bootstrap: Vec<offbeat_core::EndpointId> = self
                .inner
                .connection_manager
                .as_ref()
                .map(|cm| {
                    cm.bootstrap_peers(&festival_id, 8)
                        .into_iter()
                        .filter_map(|p| p.endpoint_id.parse().ok())
                        .collect()
                })
                .unwrap_or_default();
            let mut gm_locked = gm.lock().await;
            for (_topic_str, topic_id) in &chat_topics {
                // Public festival chat topics — lower dial priority than groups.
                gm_locked
                    .subscribe(*topic_id, &festival_id, false, bootstrap.clone())
                    .await?;
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

        let sync_orchestrator = Arc::clone(&self.inner.sync_orchestrator);
        let doc_manager = Arc::clone(&self.inner.doc_manager);
        let notifier = Arc::clone(&self.inner.notifier);

        // Ensure the festival public key is registered with the orchestrator
        let festival_pk = self.inner.festival_public_keys.get(&festival_id).copied()
            .ok_or_else(|| anyhow::anyhow!(
                "no public key for festival {festival_id} — call set_festival_public_key first"
            ))?;
        sync_orchestrator.set_festival_public_key(&festival_id, festival_pk);

        let (sink, receive_loop) = ws_relay::connect(
            &url,
            sync_orchestrator,
            doc_manager,
            notifier,
            self.inner.connection_manager.clone(),
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

        *self.inner.ws_relay.write() = Some(Arc::new(sink));

        RUNTIME.spawn(async move {
            if let Err(e) = receive_loop.await {
                tracing::warn!("ws relay receive loop exited: {e}");
            }
        });

        Ok(())
    }

    /// Subscribe to the gossip topic for a festival and perform a state vector
    /// exchange with the DO so we only receive updates we don't already have.
    ///
    /// Registers the festival as a resource in the registry, then delegates
    /// subscribe + catch-up to the SyncOrchestrator.
    pub async fn subscribe_festival(&mut self, festival_id: String) -> anyhow::Result<()> {
        // Register the festival resource so the orchestrator knows about it
        let festival_pk = self.inner.festival_public_keys.get(&festival_id).copied()
            .ok_or_else(|| anyhow::anyhow!(
                "no public key for festival {festival_id} — call set_festival_public_key first"
            ))?;
        {
            let mut reg = self.inner.resource_registry.write()
                .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
            reg.register_festival(&festival_id, festival_pk);
        }

        // Sync via orchestrator using the WS peer
        if let Some(ws) = { self.inner.ws_relay.read().clone() } {
            self.inner.sync_orchestrator.sync_with_peer(ws.as_ref()).await?;
        }

        Ok(())
    }

    /// Subscribe to all group topics for a festival and sync state.
    ///
    /// Loads groups from SQLite, registers their resources (state + chat),
    /// and triggers a sync via the WS relay if connected.
    pub async fn subscribe_groups(&mut self, festival_id: String) -> anyhow::Result<()> {
        let groups = self.inner.db.load_groups(&festival_id)?;
        if groups.is_empty() {
            return Ok(());
        }
        let pairs: Vec<(String, [u8; 32])> = groups
            .iter()
            .filter_map(|g| {
                let key: [u8; 32] = g.2.clone().try_into().ok()?;
                Some((g.0.clone(), key))
            })
            .collect();
        {
            let mut reg = self
                .inner
                .resource_registry
                .write()
                .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
            reg.register_groups(&pairs);
        }
        // Populate the key cache so incoming messages can be decoded
        for (group_id, key) in &pairs {
            self.inner.sync_orchestrator.cache_group_key(group_id, *key);
        }
        if let Some(ws) = { self.inner.ws_relay.read().clone() } {
            self.inner
                .sync_orchestrator
                .sync_with_peer(ws.as_ref())
                .await?;
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
        use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;
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

        let gossip_msg = GossipMessage::Chat(chat);
        let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
        let bytes = encode_gossip_message(&gossip_msg);

        if let Some(gm) = &self.inner.gossip_manager {
            let _ = gm.lock().await.broadcast(topic_id, bytes).await;
        }

        if let Some(ws) = { self.inner.ws_relay.read().clone() } {
            let _ = ws.send_gossip(&topic, &envelope).await;
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

    /// Create a new group, register resources, subscribe, and broadcast
    /// the initial state. Returns the group ID + shareable invite payload.
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

        // Register resources for the new group
        let group_key = self
            .inner
            .db
            .load_group_key(&result.group_id)?
            .ok_or_else(|| anyhow::anyhow!("group key not found after create"))?;
        {
            let mut reg = self
                .inner
                .resource_registry
                .write()
                .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
            reg.register_groups(&[(result.group_id.clone(), group_key)]);
        }
        self.inner
            .sync_orchestrator
            .cache_group_key(&result.group_id, group_key);

        // Subscribe + sync via WS relay if connected
        if let Some(ws) = { self.inner.ws_relay.read().clone() } {
            self.inner
                .sync_orchestrator
                .sync_with_peer(ws.as_ref())
                .await?;
        }

        // Notify local watchers so the UI picks up the new group state
        let doc_id = format!("group/{}/state", result.group_id);
        self.inner.notifier.notify_doc(&doc_id);

        // Broadcast initial state as GroupUpdate
        {
            let doc_id = format!("group/{}/state", result.group_id);
            // Ensure doc is created so encode_diff works
            self.inner.doc_manager.get_or_create(&doc_id);
            // Encode full state as diff from empty SV
            let diff = self.inner.doc_manager.encode_diff(&doc_id, &[])?;
            let encrypted = offbeat_core::crypto::encrypt(&group_key, &diff)?;

            use offbeat_core::gossip_manager::{encode_gossip_message, GossipMessage};
            use offbeat_core::proto::GossipEnvelope;
            let gossip_msg = GossipMessage::GroupUpdate {
                doc_id: doc_id.clone(),
                encrypted: encrypted.clone(),
                group_key,
            };
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let _ = gm.lock().await.broadcast(topic, bytes).await;
            }
            if let Some(ws) = { self.inner.ws_relay.read().clone() } {
                let topic_str = format!("group/{}/state", result.group_id);
                let _ = ws.send_gossip(&topic_str, &envelope).await;
            }
        }

        Ok(GroupCreateResultDto {
            group_id: result.group_id,
            festival_id: result.festival_id,
            invite_payload: result.invite_payload,
        })
    }

    /// Join an existing group, register resources, subscribe, and trigger
    /// SV exchange + chat catchup.
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

        // Register resources for the joined group
        {
            let mut reg = self
                .inner
                .resource_registry
                .write()
                .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
            reg.register_groups(&[(result.group_id.clone(), result.group_key)]);
        }
        self.inner
            .sync_orchestrator
            .cache_group_key(&result.group_id, result.group_key);

        // Subscribe + sync (SV exchange + chat catchup) via WS relay
        if let Some(ws) = { self.inner.ws_relay.read().clone() } {
            self.inner
                .sync_orchestrator
                .sync_with_peer(ws.as_ref())
                .await?;
        }

        // Notify local watchers so the UI updates immediately
        self.inner
            .notifier
            .notify_doc(&format!("group/{}/state", result.group_id));

        Ok(GroupJoinResultDto {
            group_id: result.group_id,
            festival_id: result.festival_id,
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
        // Notify local watchers so the UI updates immediately
        self.inner.notifier.notify_doc(&format!("group/{group_id}/state"));
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

        // Notify local watchers so the UI updates immediately
        self.inner.notifier.notify_doc(&format!("group/{group_id}/state"));

        if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;
            let gossip_msg = GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}/state"),
                encrypted: encrypted.clone(),
                group_key,
            };
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let _ = gm.lock().await.broadcast(topic, bytes.clone()).await;
            }
            if let Some(ws) = { self.inner.ws_relay.read().clone() } {
                let topic_str = format!("group/{group_id}/state");
                let _ = ws.send_gossip(&topic_str, &envelope).await;
            }
            let resource_id = format!("group/{group_id}/state");
            self.inner.notifier.record_sent(&resource_id);
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

        // Notify local watchers so the UI updates immediately
        self.inner.notifier.notify_doc(&format!("group/{group_id}/state"));

        if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;
            let gossip_msg = GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}/state"),
                encrypted: encrypted.clone(),
                group_key,
            };
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let _ = gm.lock().await.broadcast(topic, bytes.clone()).await;
            }
            if let Some(ws) = { self.inner.ws_relay.read().clone() } {
                let topic_str = format!("group/{group_id}/state");
                let _ = ws.send_gossip(&topic_str, &envelope).await;
            }
            let resource_id = format!("group/{group_id}/state");
            self.inner.notifier.record_sent(&resource_id);
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

        // Notify local watchers so the UI updates immediately
        self.inner.notifier.notify_doc(&format!("group/{group_id}/state"));

        if let Some(group_key) = self.inner.db.load_group_key(&group_id)? {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;
            let gossip_msg = GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}/state"),
                encrypted: encrypted.clone(),
                group_key,
            };
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);

            if let Some(gm) = &self.inner.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                let _ = gm.lock().await.broadcast(topic, bytes.clone()).await;
            }
            if let Some(ws) = { self.inner.ws_relay.read().clone() } {
                let topic_str = format!("group/{group_id}/state");
                let _ = ws.send_gossip(&topic_str, &envelope).await;
            }
            let resource_id = format!("group/{group_id}/state");
            self.inner.notifier.record_sent(&resource_id);
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
            // Emit initial state (subscribe happened before this, so no race)
            {
                let lineup = read_lineup_from_doc(&doc_manager_clone, &doc_id_clone);
                let sink = sink_clone.lock().await;
                let _ = sink.add(lineup);
            }

            // Check if data changed during the initial load
            if rx.has_changed().unwrap_or(false) {
                let lineup = read_lineup_from_doc(&doc_manager_clone, &doc_id_clone);
                let sink = sink_clone.lock().await;
                if sink.add(lineup).is_err() {
                    return;
                }
            }

            // Watch for subsequent changes
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let lineup = read_lineup_from_doc(&doc_manager_clone, &doc_id_clone);
                let sink = sink_clone.lock().await;
                if sink.add(lineup).is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    /// Watch weather forecast — emits current forecast, then updates on changes.
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_weather(
        &self,
        festival_id: String,
        sink: StreamSink<Option<WeatherForecastDto>>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let doc_id = format!("festival/{festival_id}/state");
        let doc_manager = Arc::clone(&self.inner.doc_manager);
        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_doc(&doc_id);

        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);
        let doc_manager_clone = Arc::clone(&doc_manager);
        let doc_id_clone = doc_id.clone();

        RUNTIME.spawn(async move {
            // Emit initial state
            {
                let weather = read_weather_from_doc(&doc_manager_clone, &doc_id_clone);
                let sink = sink_clone.lock().await;
                let _ = sink.add(weather);
            }

            if rx.has_changed().unwrap_or(false) {
                let weather = read_weather_from_doc(&doc_manager_clone, &doc_id_clone);
                let sink = sink_clone.lock().await;
                if sink.add(weather).is_err() {
                    return;
                }
            }

            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let weather = read_weather_from_doc(&doc_manager_clone, &doc_id_clone);
                let sink = sink_clone.lock().await;
                if sink.add(weather).is_err() {
                    break;
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

        let doc_id = format!("group/{group_id}/state");
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

    // -----------------------------------------------------------------------
    // Transport methods
    // -----------------------------------------------------------------------

    /// Start BLE auto-connection background tasks.
    ///
    /// Spawns discovery tick, reconnect tick, and gossip event pump tasks that
    /// transition BLE-discovered peers to gossip-connected. Call after
    /// subscribing to festival/group topics.
    pub fn start_ble_sync(&mut self) {
        let _guard = RUNTIME.enter();
        // Only start if BLE transport + gossip + connection manager are all present
        let Some(ble) = self.inner.ble_transport.clone() else {
            tracing::debug!("start_ble_sync: no BLE transport, skipping");
            return;
        };
        let Some(gm) = self.inner.gossip_manager.clone() else {
            tracing::debug!("start_ble_sync: no gossip manager, skipping");
            return;
        };
        let Some(cm) = self.inner.connection_manager.clone() else {
            tracing::debug!("start_ble_sync: no connection manager, skipping");
            return;
        };

        // Don't start twice
        if !self.ble_task_handles.is_empty() {
            tracing::debug!("start_ble_sync: already running");
            return;
        }

        let so = std::sync::Arc::clone(&self.inner.sync_orchestrator);
        let endpoint = self.inner.endpoint.clone();
        let doc_manager = std::sync::Arc::clone(&self.inner.doc_manager);
        // Start the auto-subscription manager FIRST so registered festival/group
        // resources actually issue an iroh-gossip `Join` (proto-level topic
        // membership). Without this the node only syncs state via the WS relay
        // and silently drops incoming peer `Join`s — so the P2P/BLE gossip mesh
        // never forms. (The headless CLI gets this via OffbeatNode::spawn_ble_sync;
        // the bridge must wire it up explicitly.)
        let mut handles = vec![so.clone().spawn_subscription_manager()];
        handles.extend(offbeat_core::ble_sync::spawn_ble_connection_tasks(
            ble, gm, cm, so, endpoint, doc_manager,
        ));
        self.ble_task_handles = handles;
        tracing::info!("subscription manager + BLE auto-connection tasks started");
    }

    /// Stop the BLE background tasks.
    pub fn stop_ble_sync(&mut self) {
        for handle in self.ble_task_handles.drain(..) {
            handle.abort();
        }
        tracing::info!("BLE auto-connection tasks stopped");
    }

    /// Explicitly trigger a proactive BLE connection to a device for verification or sync.
    pub fn connect_peer(&self, device_id: String) {
        let _guard = RUNTIME.enter();
        if let Some(ref ble) = self.inner.ble_transport {
            ble.connect(iroh_ble_transport::DeviceId::from(device_id));
        }
    }

    /// Manually trigger a gossip join nudge for all currently visible BLE peers.
    /// Useful for breaking sync deadlocks.
    pub async fn nudge_gossip(&self) -> anyhow::Result<()> {
        let ble = self.inner.ble_transport.clone();
        let gm_arc = self.inner.gossip_manager.clone();

        RUNTIME
            .spawn(async move {
                let Some(ble) = ble.as_ref() else {
                    return Ok(());
                };
                let Some(gm_arc) = gm_arc.as_ref() else {
                    return Ok(());
                };

                let targets: Vec<iroh_base::EndpointId> = ble
                    .snapshot_peers()
                    .into_iter()
                    .filter_map(|p| p.verified_endpoint)
                    .collect();

                if !targets.is_empty() {
                    let gm = gm_arc.lock().await;
                    gm.join_peers_all(targets).await;
                    tracing::info!("manually nudged gossip for verified BLE peers");
                }
                Ok::<(), anyhow::Error>(())
            })
            .await?
    }

    /// Cycle the BLE transport (stop then start).
    pub async fn restart_ble(&mut self) -> anyhow::Result<()> {
        self.stop_ble_sync();
        // Sleep on the runtime to ensure reactor is present
        RUNTIME
            .spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            })
            .await?;
        self.start_ble_sync();
        tracing::info!("manually restarted BLE transport");
        Ok(())
    }


    /// Get a snapshot of transport status (no rate computation).
    pub fn get_transport_status(&self) -> TransportStatusDto {
        snapshot_transport(&self.inner)
    }

    /// Return the number of active direct peers (those with gossip status "active").
    pub fn get_peer_count(&self) -> anyhow::Result<u32> {
        match &self.inner.connection_manager {
            Some(cm) => Ok(cm.active_peer_count()),
            None => Ok(0),
        }
    }

    /// Return a snapshot of all known peers for the UI.
    pub fn get_peer_list(&self) -> anyhow::Result<Vec<PeerStatusInfo>> {
        match &self.inner.connection_manager {
            Some(cm) => Ok(cm
                .peer_snapshot()
                .into_iter()
                .map(peer_entry_to_dto)
                .collect()),
            None => Ok(vec![]),
        }
    }

    /// Watch the peer list — polls every second and emits whenever the
    /// snapshot changes (peer count or any entry status).
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_peer_list(
        &self,
        sink: StreamSink<Vec<PeerStatusInfo>>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let cm = self.inner.connection_manager.clone();
        let sink = Arc::new(Mutex::new(sink));

        RUNTIME.spawn(async move {
            let mut prev_snapshot: Vec<String> = vec![];

            loop {
                let current: Vec<PeerStatusInfo> = match &cm {
                    Some(cm) => cm
                        .peer_snapshot()
                        .into_iter()
                        .map(peer_entry_to_dto)
                        .collect(),
                    None => vec![],
                };

                // Build a simple fingerprint to detect changes
                let fingerprint: Vec<String> = current
                    .iter()
                    .map(|p| format!("{}:{}:{}", p.endpoint_id, p.source, p.status))
                    .collect();

                if fingerprint != prev_snapshot {
                    prev_snapshot = fingerprint;
                    let sink = sink.lock().await;
                    if sink.add(current).is_err() {
                        break;
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        Ok(())
    }

    /// Watch transport status — emits relay + BLE state with bandwidth
    /// rates computed by diffing cumulative counters every second.
    #[flutter_rust_bridge::frb(stream_dart_await)]
    pub fn watch_transport_status(
        &self,
        sink: StreamSink<TransportStatusDto>,
    ) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let ble = self.inner.ble_transport.clone();
        let ws = self.inner.ws_relay.clone();
        let sink = Arc::new(Mutex::new(sink));

        RUNTIME.spawn(async move {
            let mut prev_relay_tx: u64 = 0;
            let mut prev_relay_rx: u64 = 0;
            let mut prev_ble_tx: u64 = 0;
            let mut prev_ble_rx: u64 = 0;

            loop {
                // Relay stats
                let relay = match &*ws.read() {
                    Some(ws) => {
                        let tx = ws.tx_bytes();
                        let rx = ws.rx_bytes();
                        let dto = RelayStatusDto {
                            connected: ws.is_connected(),
                            authenticated: ws.is_authenticated(),
                            tx_bytes_per_sec: tx.saturating_sub(prev_relay_tx),
                            rx_bytes_per_sec: rx.saturating_sub(prev_relay_rx),
                        };
                        prev_relay_tx = tx;
                        prev_relay_rx = rx;
                        dto
                    }
                    None => RelayStatusDto {
                        connected: false,
                        authenticated: false,
                        tx_bytes_per_sec: 0,
                        rx_bytes_per_sec: 0,
                    },
                };

                // BLE stats
                let ble_status = match &ble {
                    Some(ble) => {
                        let metrics = ble.metrics();
                        let peers: Vec<TransportPeerDto> = ble
                            .snapshot_peers()
                            .into_iter()
                            .map(|p| TransportPeerDto {
                                device_id: p.device_id.to_string(),
                                phase: format!("{:?}", p.phase),
                                connect_path: p.connect_path.map(|c| format!("{c:?}")),
                                verified_endpoint: p.verified_endpoint.map(|e| e.to_string()),
                                consecutive_failures: p.consecutive_failures,
                                key_prefix: p.prefix.map(|pr| hex::encode(pr)),
                            })
                            .collect();
                        let dto = BleStatusDto {
                            active: true,
                            peer_count: peers.len() as u32,
                            tx_bytes_per_sec: metrics.tx_bytes.saturating_sub(prev_ble_tx),
                            rx_bytes_per_sec: metrics.rx_bytes.saturating_sub(prev_ble_rx),
                            retransmits: metrics.retransmits,
                            peers,
                            advertising_beacons: ble
                                .advertising_beacons()
                                .iter()
                                .map(|u| u.to_string())
                                .collect(),
                        };
                        prev_ble_tx = metrics.tx_bytes;
                        prev_ble_rx = metrics.rx_bytes;
                        dto
                    }
                    None => BleStatusDto {
                        active: false,
                        peer_count: 0,
                        tx_bytes_per_sec: 0,
                        rx_bytes_per_sec: 0,
                        retransmits: 0,
                        peers: vec![],
                        advertising_beacons: vec![],
                    },
                };

                let status = TransportStatusDto {
                    relay,
                    ble: ble_status,
                };
                let sink = sink.lock().await;
                if sink.add(status).is_err() {
                    break;
                }
                drop(sink);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
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
