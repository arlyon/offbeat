pub mod dto;

use crate::frb_generated::StreamSink;
use base64::Engine as _;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::runtime::Runtime;

// Re-export DTOs and utilities for convenience
pub use dto::*;

// Re-export types used in public function signatures for FRB
pub use offbeat_core::OffbeatNode;
use offbeat_core::auth;
pub use offbeat_core::connection_manager::PeerEntry;
pub use offbeat_core::doc_manager::DocManager;
use offbeat_core::gossip_manager::GossipMessage;
pub use offbeat_core::notifier::SyncStatus;
use offbeat_core::proto::GossipEnvelope;

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
            services: device
                .services
                .into_iter()
                .map(|uuid| uuid.to_string())
                .collect(),
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
        DEFAULT_TTL, MeshtasticReassembly, OffbeatSyncFrame, encode_to_radio_private_app,
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
        applied_group_chats: 0,
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
    use offbeat_core::transport::meshtastic::{MeshtasticPacket, decode_from_radio_private_app};

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

fn group_id_from_chat_topic(topic: &str) -> Option<&str> {
    let group_id = topic.strip_prefix("group/")?.strip_suffix("/chat")?;
    (!group_id.is_empty() && !group_id.contains('/')).then_some(group_id)
}

#[cfg(target_os = "android")]
fn first_eight_uuid_bytes(uuid: uuid::Uuid) -> [u8; 8] {
    let mut out = [0u8; 8];
    out.copy_from_slice(&uuid.as_bytes()[..8]);
    out
}

#[cfg(target_os = "android")]
fn service_summaries(services: &[blew::gatt::GattService]) -> Vec<String> {
    services
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
        .collect()
}

#[cfg(target_os = "android")]
fn group_key_from_vec(key: Vec<u8>) -> anyhow::Result<[u8; 32]> {
    key.try_into()
        .map_err(|key: Vec<u8>| anyhow::anyhow!("group key is {} bytes; expected 32", key.len()))
}

#[cfg(target_os = "android")]
fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
pub struct FestivalRegistryCacheStore {
    db_path: std::path::PathBuf,
}

impl FestivalRegistryCacheStore {
    /// Validate and retain the SQLite path without constructing any transports
    /// or keeping a connection open across database recreation (for logout).
    pub fn open(db_path: String) -> anyhow::Result<Self> {
        let db_path = std::path::PathBuf::from(db_path);
        offbeat_core::db::Database::new(&db_path)?;
        Ok(Self { db_path })
    }

    pub fn replace(
        &self,
        payload_json: String,
        fetched_at: String,
        request_token: String,
    ) -> anyhow::Result<bool> {
        const MAX_REGISTRY_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
        if payload_json.len() > MAX_REGISTRY_PAYLOAD_BYTES {
            anyhow::bail!("festival registry response exceeds 2 MiB");
        }
        let festivals: Vec<offbeat_core::types::Festival> = serde_json::from_str(&payload_json)
            .map_err(|error| anyhow::anyhow!("invalid festival registry JSON: {error}"))?;
        offbeat_core::db::Database::new(&self.db_path)?.replace_festival_registry_cache(
            &festivals,
            &fetched_at,
            &request_token,
        )
    }

    pub fn load(&self) -> anyhow::Result<Option<FestivalRegistryCacheDto>> {
        offbeat_core::db::Database::new(&self.db_path)?
            .load_festival_registry_cache()?
            .map(|cache| {
                Ok(FestivalRegistryCacheDto {
                    payload_json: serde_json::to_string(&cache.festivals)?,
                    fetched_at: cache.fetched_at,
                    request_token: cache.request_token,
                })
            })
            .transpose()
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct AppNode {
    inner: OffbeatNode,
    relay_festival_id: Arc<std::sync::RwLock<Option<String>>>,
    pending_group_retries: Arc<std::sync::Mutex<std::collections::HashSet<i64>>>,
    group_retry_generations: Arc<std::sync::Mutex<std::collections::HashMap<String, u64>>>,
    relay_task_handle: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    group_publish_locks:
        Arc<std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    /// Join handles for BLE connection background tasks.
    ble_task_handles: Vec<tokio::task::JoinHandle<()>>,
    /// Bridge-owned retries, acknowledgements, and stream watcher tasks.
    background_tasks: offbeat_core::task_scope::TaskScope,
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
            relay_festival_id: Arc::new(std::sync::RwLock::new(None)),
            pending_group_retries: Arc::new(
                std::sync::Mutex::new(std::collections::HashSet::new()),
            ),
            group_retry_generations: Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            relay_task_handle: Arc::new(std::sync::Mutex::new(None)),
            group_publish_locks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            ble_task_handles: Vec::new(),
            background_tasks: offbeat_core::task_scope::TaskScope::new(),
        })
    }

    /// Create an in-memory node (useful for testing).
    pub fn create_in_memory() -> anyhow::Result<AppNode> {
        let _guard = RUNTIME.enter();
        let inner = OffbeatNode::new_in_memory()?;
        Ok(AppNode {
            inner,
            relay_festival_id: Arc::new(std::sync::RwLock::new(None)),
            pending_group_retries: Arc::new(
                std::sync::Mutex::new(std::collections::HashSet::new()),
            ),
            group_retry_generations: Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            relay_task_handle: Arc::new(std::sync::Mutex::new(None)),
            group_publish_locks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            ble_task_handles: Vec::new(),
            background_tasks: offbeat_core::task_scope::TaskScope::new(),
        })
    }

    fn spawn_background_task<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.background_tasks.spawn(future);
    }

    /// Return the set IDs that are starred for the given festival.
    pub fn get_stars(&self, festival_id: String) -> anyhow::Result<Vec<String>> {
        self.inner.db.get_stars(&festival_id)
    }

    /// Atomically replace the app-side cache of the server-authoritative registry.
    pub fn replace_festival_registry_cache(
        &self,
        payload_json: String,
        fetched_at: String,
        request_token: String,
    ) -> anyhow::Result<bool> {
        const MAX_REGISTRY_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
        if payload_json.len() > MAX_REGISTRY_PAYLOAD_BYTES {
            anyhow::bail!("festival registry response exceeds 2 MiB");
        }
        let festivals: Vec<offbeat_core::types::Festival> = serde_json::from_str(&payload_json)
            .map_err(|error| anyhow::anyhow!("invalid festival registry JSON: {error}"))?;
        self.inner
            .db
            .replace_festival_registry_cache(&festivals, &fetched_at, &request_token)
    }

    /// Load the cached registry, if a successful server fetch has been persisted.
    pub fn get_festival_registry_cache(&self) -> anyhow::Result<Option<FestivalRegistryCacheDto>> {
        self.inner
            .db
            .load_festival_registry_cache()?
            .map(|cache| {
                Ok(FestivalRegistryCacheDto {
                    payload_json: serde_json::to_string(&cache.festivals)?,
                    fetched_at: cache.fetched_at,
                    request_token: cache.request_token,
                })
            })
            .transpose()
    }

    /// Toggle a personal star and reconcile the resulting schedule into every
    /// encrypted group for this festival.
    pub async fn toggle_star(&self, festival_id: String, set_id: String) -> anyhow::Result<bool> {
        let starred = self.inner.db.toggle_star(&festival_id, &set_id)?;
        if let Err(error) = self.reconcile_shared_stars(&festival_id).await {
            tracing::warn!(%festival_id, %error, "personal star saved; group reconciliation will retry");
        }
        Ok(starred)
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
        let Some(stored_festival_id) = self.inner.db.load_group_festival_id(&group_id)? else {
            return Ok(None);
        };
        if stored_festival_id != festival_id {
            anyhow::bail!("group does not belong to the requested festival");
        }
        let key = self.inner.db.load_group_key(&group_id)?;
        Ok(key.map(|key| {
            let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
            format!("offbeat://group/{stored_festival_id}/{group_id}/{b64}")
        }))
    }

    /// Delete a group by ID.
    pub fn delete_group(&self, id: String) -> anyhow::Result<()> {
        self.inner.db.delete_group(&id)
    }

    fn relay_matches_festival(&self, festival_id: &str) -> bool {
        self.relay_festival_id
            .read()
            .is_ok_and(|active| active.as_deref() == Some(festival_id))
    }

    fn ensure_group_chat_access(&self, topic: &str) -> anyhow::Result<()> {
        if let Some(group_id) = group_id_from_chat_topic(topic)
            && self.inner.db.load_group_key(group_id)?.is_none()
        {
            anyhow::bail!("group membership not found");
        }
        Ok(())
    }

    /// Fetch chat messages for a topic with pagination.
    pub fn get_chat_messages(
        &self,
        topic: String,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<ChatMessageDto>> {
        self.ensure_group_chat_access(&topic)?;
        let msgs = self
            .inner
            .db
            .get_recent_chat_messages(&topic, limit, offset)?;
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
                trust: chat_trust_label(m.trust),
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
        let display_name =
            auth::get_display_name(&self.inner.db)?.unwrap_or_else(|| user_id.clone());

        let (msg, topic_id) = self.inner.chat_manager.send_festival_chat(
            &festival_id,
            stage_id.as_deref(),
            &user_id,
            &display_name,
            &text,
            &signing_key,
        )?;

        {
            use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
            use offbeat_core::proto::GossipEnvelope;
            let gossip_msg = GossipMessage::Chat(msg.clone());
            let envelope = GossipEnvelope::from_gossip_message(&gossip_msg);
            let bytes = encode_gossip_message(&gossip_msg);
            let writer_key = signing_key.verifying_key().to_bytes();
            let proof = self
                .inner
                .db
                .get_chat_author_proof(&writer_key)?
                .map(|proof| {
                    let message = GossipMessage::ChatAuthorProof {
                        writer_key: proof.writer_key,
                        attestation_message: proof.attestation_message,
                        attestation_signature: proof.attestation_signature,
                        issuer: proof.issuer,
                    };
                    (
                        GossipEnvelope::from_gossip_message(&message),
                        encode_gossip_message(&message),
                    )
                });

            if let Some(gm) = &self.inner.gossip_manager {
                let mut gm = gm.lock().await;
                if let Some((_, proof_bytes)) = &proof {
                    let _ = gm.broadcast(topic_id, proof_bytes.clone()).await;
                }
                let _ = gm.broadcast(topic_id, bytes).await;
            }

            if self.relay_matches_festival(&festival_id)
                && let Some(ws) = { self.inner.ws_relay.read().clone() }
            {
                let topic_str = format!(
                    "festival/{}/chat/{}",
                    festival_id,
                    stage_id.as_deref().unwrap_or("campsite")
                );
                if let Some((proof_envelope, _)) = &proof {
                    let _ = ws.send_gossip(&topic_str, proof_envelope).await;
                }
                let db = Arc::clone(&self.inner.db);
                let message_id = msg.id.clone();
                self.spawn_background_task(async move {
                    if ws.send_gossip_confirmed(&topic_str, &envelope).await.is_ok()
                        && let Err(error) = db.delete_pending_public_chat(&message_id)
                    {
                        tracing::warn!(%error, %message_id, "public chat acknowledgement was not recorded");
                    }
                });
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
            trust: chat_trust_label(msg.trust),
        })
    }

    async fn flush_pending_public_chats(&self, festival_id: &str) -> anyhow::Result<()> {
        if !self.relay_matches_festival(festival_id) {
            return Ok(());
        }
        let Some(ws) = self.inner.ws_relay.read().clone() else {
            return Ok(());
        };
        for pending in self.inner.db.load_pending_public_chats(festival_id, 100)? {
            if !offbeat_core::signing::verify_public_chat_message(&pending.message) {
                tracing::warn!(message_id = %pending.message_id, "corrupt pending public chat retained");
                continue;
            }
            let writer_key: [u8; 32] = pending
                .message
                .writer_key
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("pending public chat writer key is malformed"))?;
            if let Some(proof) = self.inner.db.get_chat_author_proof(&writer_key)? {
                let proof_message = GossipMessage::ChatAuthorProof {
                    writer_key: proof.writer_key,
                    attestation_message: proof.attestation_message,
                    attestation_signature: proof.attestation_signature,
                    issuer: proof.issuer,
                };
                let proof_envelope = GossipEnvelope::from_gossip_message(&proof_message);
                let _ = ws
                    .send_gossip(&pending.message.topic, &proof_envelope)
                    .await;
            }
            let envelope =
                GossipEnvelope::from_gossip_message(&GossipMessage::Chat(pending.message.clone()));
            if ws
                .send_gossip_confirmed(&pending.message.topic, &envelope)
                .await
                .is_err()
            {
                break;
            }
            self.inner
                .db
                .delete_pending_public_chat(&pending.message_id)?;
        }
        Ok(())
    }

    /// Send an encrypted group chat message and broadcast it via gossip if
    /// networking is active. Returns the persisted message.
    pub async fn send_group_chat(
        &self,
        group_id: String,
        text: String,
    ) -> anyhow::Result<ChatMessageDto> {
        use offbeat_core::auth;
        let festival_id = self
            .inner
            .db
            .load_group_festival_id(&group_id)?
            .filter(|festival_id| !festival_id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("group has no verified festival scope"))?;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let display_name =
            auth::get_display_name(&self.inner.db)?.unwrap_or_else(|| user_id.clone());

        let (encrypted, topic_id) =
            self.inner
                .chat_manager
                .send_group_chat(&group_id, &user_id, &display_name, &text)?;

        let stored =
            self.inner
                .db
                .get_recent_chat_messages(&format!("group/{group_id}/chat"), 1, 0)?;
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

            if self.relay_matches_festival(&festival_id)
                && let Some(ws) = { self.inner.ws_relay.read().clone() }
            {
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
            trust: chat_trust_label(msg.trust),
        })
    }

    /// Send a compact encrypted group chat through a selected Meshtastic radio.
    pub async fn meshtastic_send_group_chat(
        &self,
        device_id: String,
        group_id: String,
        text: String,
    ) -> anyhow::Result<MeshtasticDebugReportDto> {
        #[cfg(target_os = "android")]
        {
            use blew::central::{Central, WriteType};
            use blew::types::DeviceId;
            use offbeat_core::auth;
            use offbeat_core::crypto;
            use offbeat_core::transport::meshtastic::{
                CompactGroupChat, DEFAULT_TTL, OffbeatSyncFrame, encode_to_radio_private_app,
                group_chat_topic_tag,
            };
            use offbeat_core::transport::profile::SyncPayloadKind;
            use offbeat_core::types::ChatMessage;
            use tokio::time::Duration;
            use uuid::Uuid;

            let group_key = self
                .inner
                .db
                .load_group_key(&group_id)?
                .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;
            let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
            let user_id = auth::get_user_id(&signing_key);
            let display_name =
                auth::get_display_name(&self.inner.db)?.unwrap_or_else(|| user_id.clone());
            let topic = format!("group/{group_id}/chat");
            let message_uuid = Uuid::new_v4();
            let timestamp_secs = now_unix_secs();
            let message = self.inner.db.save_local_chat_message(ChatMessage {
                id: message_uuid.to_string(),
                user_id: user_id.clone(),
                display_name: display_name.clone(),
                text: text.clone(),
                topic: topic.clone(),
                stage_id: None,
                timestamp: format!("{timestamp_secs}Z"),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: offbeat_core::types::ChatTrust::Unverified,
            })?;
            let compact = CompactGroupChat {
                message_uuid: *message_uuid.as_bytes(),
                user_id,
                display_name,
                text,
                writer_seq: message.writer_seq,
                logical_time: message.logical_time,
                timestamp_secs,
            };
            let encrypted_body = crypto::encrypt(&group_key, &compact.encode()?)?;
            let mut message_id = [0u8; 8];
            message_id.copy_from_slice(&message_uuid.as_bytes()[..8]);
            let frame = OffbeatSyncFrame::new(
                SyncPayloadKind::GroupChat,
                group_chat_topic_tag(&group_key),
                message_id,
                DEFAULT_TTL,
                encrypted_body,
            )?;

            self.inner.notifier.record_sent(&topic);
            self.inner.notifier.notify_chat(&topic);

            let central: Central = Central::new().await?;
            let device = DeviceId::from(device_id.clone());
            let service_uuid = Uuid::parse_str(MESHTASTIC_SERVICE_UUID)?;
            let to_radio_uuid = Uuid::parse_str(MESHTASTIC_TO_RADIO_CHAR_UUID)?;
            central
                .start_scan(blew::central::ScanFilter {
                    services: vec![service_uuid],
                    mode: blew::central::ScanMode::LowLatency,
                })
                .await?;
            tokio::time::sleep(Duration::from_millis(750)).await;
            let _ = central.stop_scan().await;
            central.connect(&device).await?;
            let services = central.discover_services(&device).await?;
            let service_summaries = service_summaries(&services);
            let mtu = central.mtu(&device).await;
            let mut sent_fragments = 0u32;
            for private_payload in frame.encode_private_app_payloads()? {
                let to_radio = encode_to_radio_private_app(
                    private_payload,
                    SyncPayloadKind::GroupChat.priority(),
                    DEFAULT_TTL,
                    false,
                )?;
                central
                    .write_characteristic(&device, to_radio_uuid, to_radio, WriteType::WithResponse)
                    .await?;
                sent_fragments += 1;
            }
            let _ = central.disconnect(&device).await;
            Ok(MeshtasticDebugReportDto {
                device_id,
                connected: true,
                mtu,
                services: service_summaries,
                sent_fragments,
                raw_from_radio_count: 0,
                private_app_count: 0,
                applied_group_chats: 0,
                received_frames: Vec::new(),
                events: vec![
                    format!("sent_group_chat:{group_id}"),
                    format!("sent_fragments:{sent_fragments}"),
                ],
            })
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (device_id, group_id, text);
            anyhow::bail!("Meshtastic group chat send is currently wired for Android builds only")
        }
    }

    /// Listen for Meshtastic group chat frames and apply matching local groups.
    pub async fn meshtastic_listen_apply_group_chats(
        &self,
        device_id: String,
        festival_id: String,
        listen_ms: u32,
    ) -> anyhow::Result<MeshtasticDebugReportDto> {
        #[cfg(target_os = "android")]
        {
            use blew::central::{Central, CentralEvent};
            use blew::types::DeviceId;
            use offbeat_core::crypto;
            use offbeat_core::transport::meshtastic::{
                CompactGroupChat, MeshtasticPacket, MeshtasticReassembly,
                decode_from_radio_private_app, group_chat_topic_tag,
            };
            use offbeat_core::transport::profile::SyncPayloadKind;
            use offbeat_core::types::ChatMessage;
            use std::collections::HashMap;
            use tokio::time::{Duration, Instant, timeout};
            use tokio_stream::StreamExt;
            use uuid::Uuid;

            let mut group_by_tag: HashMap<[u8; 8], (String, [u8; 32])> = HashMap::new();
            for (group_id, _name, key) in self.inner.db.load_groups(&festival_id)? {
                let group_key = group_key_from_vec(key)?;
                group_by_tag.insert(group_chat_topic_tag(&group_key), (group_id, group_key));
            }
            if group_by_tag.is_empty() {
                anyhow::bail!("no local groups for festival {festival_id}");
            }

            let central: Central = Central::new().await?;
            let device = DeviceId::from(device_id.clone());
            let service_uuid = Uuid::parse_str(MESHTASTIC_SERVICE_UUID)?;
            let from_num_uuid = Uuid::parse_str(MESHTASTIC_FROM_NUM_CHAR_UUID)?;
            let from_radio_uuid = Uuid::parse_str(MESHTASTIC_FROM_RADIO_CHAR_UUID)?;
            let mut events = central.events();
            let mut events_log = Vec::new();
            let mut raw_from_radio_count = 0u32;
            let mut private_app_count = 0u32;
            let mut applied_group_chats = 0u32;
            let mut reassembly = MeshtasticReassembly::default();
            let mut received_frames = Vec::new();

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
            let service_summaries = service_summaries(&services);
            central
                .subscribe_characteristic(&device, from_num_uuid)
                .await?;
            events_log.push("subscribed:from_num".to_string());
            let mtu = central.mtu(&device).await;
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
                        loop {
                            let raw = central
                                .read_characteristic(&device, from_radio_uuid)
                                .await?;
                            if raw.is_empty() {
                                break;
                            }
                            raw_from_radio_count += 1;
                            let Some(private_payload) = decode_from_radio_private_app(&raw)? else {
                                continue;
                            };
                            private_app_count += 1;
                            let packet = MeshtasticPacket::decode(&private_payload)?;
                            if let Some(frame) = reassembly.push(packet)? {
                                received_frames.push(MeshtasticDebugFrameDto {
                                    kind: format!("{:?}", frame.kind),
                                    topic_tag_hex: hex::encode(frame.topic_tag),
                                    message_id_hex: hex::encode(frame.message_id),
                                    body_text: None,
                                    body_hex: hex::encode(&frame.body),
                                });
                                if frame.kind != SyncPayloadKind::GroupChat {
                                    continue;
                                }
                                let Some((group_id, group_key)) =
                                    group_by_tag.get(&frame.topic_tag)
                                else {
                                    events_log.push(format!(
                                        "ignored_unknown_group:{}",
                                        hex::encode(frame.topic_tag)
                                    ));
                                    continue;
                                };
                                let plaintext = crypto::decrypt(group_key, &frame.body)?;
                                let compact = CompactGroupChat::decode(&plaintext)?;
                                let topic = format!("group/{group_id}/chat");
                                let message = ChatMessage {
                                    id: Uuid::from_bytes(compact.message_uuid).to_string(),
                                    user_id: compact.user_id,
                                    display_name: compact.display_name,
                                    text: compact.text,
                                    topic: topic.clone(),
                                    stage_id: None,
                                    timestamp: format!("{}Z", compact.timestamp_secs),
                                    writer_seq: compact.writer_seq,
                                    logical_time: compact.logical_time,
                                    writer_key: Vec::new(),
                                    signature: Vec::new(),
                                    trust: offbeat_core::types::ChatTrust::Unverified,
                                };
                                self.inner.db.save_chat_message(&message)?;
                                self.inner.notifier.record_received(&topic);
                                self.inner.notifier.notify_chat(&topic);
                                applied_group_chats += 1;
                                events_log.push(format!("applied_group_chat:{group_id}"));
                            }
                        }
                    }
                    CentralEvent::DeviceDisconnected { device_id, cause }
                        if device_id == device =>
                    {
                        events_log.push(format!("disconnected:{cause:?}"));
                        break;
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
                sent_fragments: 0,
                raw_from_radio_count,
                private_app_count,
                applied_group_chats,
                received_frames,
                events: events_log,
            })
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (device_id, festival_id, listen_ms);
            anyhow::bail!("Meshtastic group chat apply is currently wired for Android builds only")
        }
    }

    /// Return paginated chat history for a topic string.
    pub fn get_chat_history(
        &self,
        topic: String,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<ChatMessageDto>> {
        self.ensure_group_chat_access(&topic)?;
        let msgs = self
            .inner
            .db
            .get_recent_chat_messages(&topic, limit, offset)?;
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
                trust: chat_trust_label(m.trust),
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
        if let Some(relay) = { self.inner.ws_relay.read().clone() } {
            if !self.relay_matches_festival(&festival_id) {
                anyhow::bail!("active relay belongs to another festival");
            }
            relay
                .subscribe(chat_topics.iter().map(|(topic, _)| topic.clone()).collect())
                .await?;
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

        self.disconnect_relay().await;

        let sync_orchestrator = Arc::clone(&self.inner.sync_orchestrator);
        let doc_manager = Arc::clone(&self.inner.doc_manager);
        let notifier = Arc::clone(&self.inner.notifier);

        // Ensure the festival public key is registered with the orchestrator
        let festival_pk = self
            .inner
            .festival_public_keys
            .get(&festival_id)
            .copied()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no public key for festival {festival_id} — call set_festival_public_key first"
                )
            })?;
        sync_orchestrator.set_festival_public_key(&festival_id, festival_pk);

        let (sink, receive_loop) = ws_relay::connect(
            &url,
            sync_orchestrator,
            doc_manager,
            notifier,
            self.inner.connection_manager.clone(),
        )
        .await?;
        sink.enable_persistence_acknowledgements().await?;

        if let Ok(Some(attestation)) = auth::load_attestation(&self.inner.db)
            && let Ok(signing_key) = auth::generate_or_load_identity(&self.inner.db)
        {
            let pubkey_hex = auth::get_public_key_hex(&signing_key);
            if let Err(e) = sink
                .authenticate(&pubkey_hex, &attestation, &signing_key)
                .await
            {
                tracing::warn!("ws relay auth failed: {e}");
            }
        }

        *self.inner.ws_relay.write() = Some(Arc::new(sink));
        if let Ok(mut active_festival) = self.relay_festival_id.write() {
            *active_festival = Some(festival_id.clone());
        }

        let relay_task = RUNTIME.spawn(async move {
            if let Err(e) = receive_loop.await {
                tracing::warn!("ws relay receive loop exited: {e}");
            }
        });
        *self.relay_task_handle.lock().unwrap() = Some(relay_task);

        if let Err(error) = self.flush_pending_public_chats(&festival_id).await {
            tracing::warn!(%error, "queued public chat remains pending");
        }
        if let Err(error) = self.flush_pending_group_updates(&festival_id).await {
            tracing::warn!(%error, "queued group updates remain pending");
        }
        Ok(())
    }

    /// Stop the active Festival DO relay and its reconnect loop.
    pub async fn disconnect_relay(&self) {
        let relay = self.inner.ws_relay.write().take();
        if let Ok(mut active_festival) = self.relay_festival_id.write() {
            *active_festival = None;
        }
        if let Some(relay) = relay {
            relay.close().await;
        }
        let task = self.relay_task_handle.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }

    /// Stop networking and atomically remove account/private state.
    ///
    /// Public festival state and attributed public chat remain in SQLite. The
    /// non-networked replacement drops every in-memory group document and key
    /// cache before this method returns.
    pub async fn logout_preserving_public_data(&mut self) -> anyhow::Result<()> {
        // Never complete a logout that would strand this passkey offline.
        offbeat_core::auth::ensure_offline_unlock_ready(&self.inner.db)?;
        self.disconnect_relay().await;
        self.pending_group_retries.lock().unwrap().clear();
        self.group_retry_generations.lock().unwrap().clear();
        self.group_publish_locks.lock().unwrap().clear();
        self.stop_ble_sync().await;
        self.inner.ble_child_tasks.shutdown().await;
        self.background_tasks.shutdown().await;

        if let Some(router) = self.inner.router.take()
            && let Err(error) = router.shutdown().await
        {
            tracing::warn!(%error, "iroh router shutdown failed during logout");
        }
        if let Some(endpoint) = self.inner.endpoint.take() {
            endpoint.close().await;
        }
        self.inner.gossip_manager = None;
        self.inner.gossip = None;
        self.inner.ble_transport = None;
        self.inner.connection_manager = None;

        let db = Arc::clone(&self.inner.db);
        db.purge_private_state_for_logout()?;
        self.inner = OffbeatNode::new_empty_with_database(db);
        self.background_tasks = offbeat_core::task_scope::TaskScope::new();
        Ok(())
    }

    /// Subscribe to the gossip topic for a festival and perform a state vector
    /// exchange with the DO so we only receive updates we don't already have.
    ///
    /// Registers the festival as a resource in the registry, then delegates
    /// subscribe + catch-up to the SyncOrchestrator.
    pub async fn subscribe_festival(&mut self, festival_id: String) -> anyhow::Result<()> {
        // Register the festival resource so the orchestrator knows about it
        let festival_pk = self
            .inner
            .festival_public_keys
            .get(&festival_id)
            .copied()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no public key for festival {festival_id} — call set_festival_public_key first"
                )
            })?;
        {
            let mut reg = self
                .inner
                .resource_registry
                .write()
                .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
            reg.register_festival(&festival_id, festival_pk);
        }

        // Sync via orchestrator using the WS peer
        if let Some(ws) = { self.inner.ws_relay.read().clone() } {
            if !self.relay_matches_festival(&festival_id) {
                anyhow::bail!("active relay belongs to another festival");
            }
            self.inner
                .sync_orchestrator
                .sync_with_peer_for_festival(ws.as_ref(), &festival_id)
                .await?;
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
        if let Err(error) = self.reconcile_shared_stars(&festival_id).await {
            tracing::warn!(%festival_id, %error, "group schedule reconciliation will retry");
        }
        if let Err(error) = self.flush_pending_group_updates(&festival_id).await {
            tracing::warn!(%error, "queued group updates remain pending");
        }
        if self.relay_matches_festival(&festival_id)
            && let Some(ws) = { self.inner.ws_relay.read().clone() }
            && let Err(error) = self
                .inner
                .sync_orchestrator
                .sync_with_peer_for_festival(ws.as_ref(), &festival_id)
                .await
        {
            tracing::warn!(%error, "group relay catch-up will retry");
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
    pub async fn publish_chat(&self, topic: String, message: ChatMessageDto) -> anyhow::Result<()> {
        use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
        use offbeat_core::proto::GossipEnvelope;
        use offbeat_core::types::ChatMessage;

        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let chat = self.inner.db.save_local_signed_chat_message(
            ChatMessage {
                id: message.id,
                user_id,
                display_name: message.display_name,
                text: message.text,
                topic: topic.clone(),
                stage_id: message.stage_id,
                timestamp: message.timestamp,
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: offbeat_core::types::ChatTrust::Unverified,
            },
            &signing_key,
        )?;

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

        if self.relay_matches_festival(parts[1])
            && let Some(ws) = { self.inner.ws_relay.read().clone() }
        {
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
        Ok(IdentityDto {
            user_id,
            display_name,
        })
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

    /// Re-derive and activate an identity using a locally available passkey.
    pub fn unlock_identity_from_prf(&self, prf_output: Vec<u8>) -> anyhow::Result<String> {
        let arr: [u8; 32] = prf_output
            .try_into()
            .map_err(|_| anyhow::anyhow!("PRF output must be exactly 32 bytes"))?;
        let key = offbeat_core::auth::unlock_identity_from_prf(&self.inner.db, &arr)?;
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

    /// Pin the MainDO key used to verify portable public-chat attestations.
    pub fn pin_main_do_public_key(&self, public_key_hex: String) -> anyhow::Result<()> {
        offbeat_core::auth::pin_main_do_public_key(&self.inner.db, &public_key_hex)
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
        offbeat_core::auth::store_attestation(&self.inner.db, &att)?;

        // Registration may complete after the relay socket was opened. Upgrade
        // that existing session in place so queued group writes can progress.
        if let Some(relay) = self.inner.ws_relay.read().clone() {
            let signing_key = offbeat_core::auth::generate_or_load_identity(&self.inner.db)?;
            let public_key_hex = offbeat_core::auth::get_public_key_hex(&signing_key);
            self.spawn_background_task(async move {
                if let Err(error) = relay
                    .authenticate(&public_key_hex, &att, &signing_key)
                    .await
                {
                    tracing::warn!(%error, "ws relay authentication refresh failed");
                }
            });
        }
        Ok(())
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

    fn group_publish_lock(&self, group_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.group_publish_locks
            .lock()
            .unwrap()
            .entry(group_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    fn register_group_resources(&self, group_id: &str, group_key: [u8; 32]) -> anyhow::Result<()> {
        let mut registry = self
            .inner
            .resource_registry
            .write()
            .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
        registry.register_groups(&[(group_id.to_string(), group_key)]);
        drop(registry);
        self.inner
            .sync_orchestrator
            .cache_group_key(group_id, group_key);
        Ok(())
    }

    async fn reconcile_shared_stars(&self, festival_id: &str) -> anyhow::Result<()> {
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let updates = self
            .inner
            .group_manager
            .reconcile_stars_for_festival(festival_id, &user_id)
            .await?;

        for update in updates {
            self.publish_group_state_update(
                &update.group_id,
                update.group_key,
                update.encrypted_update,
            )
            .await?;
        }
        Ok(())
    }

    async fn send_group_state_update(
        &self,
        festival_id: &str,
        group_id: &str,
        group_key: [u8; 32],
        encrypted_update: Vec<u8>,
    ) -> bool {
        use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
        use offbeat_core::proto::GossipEnvelope;

        let doc_id = format!("group/{group_id}/state");
        let message = GossipMessage::GroupUpdate {
            doc_id: doc_id.clone(),
            encrypted: encrypted_update,
            group_key,
        };
        let envelope = GossipEnvelope::from_gossip_message(&message);
        let bytes = encode_gossip_message(&message);
        let mut sent = false;

        if let Some(gossip) = &self.inner.gossip_manager {
            let topic = offbeat_core::topics::group_topic(&group_key, "state");
            sent |= gossip.lock().await.broadcast(topic, bytes).await.is_ok();
        }

        let relay_accepted = self
            .send_pending_group_update(festival_id, group_id, &envelope)
            .await;
        sent |= relay_accepted;
        if sent {
            self.inner.notifier.record_sent(&doc_id);
        }
        relay_accepted
    }

    async fn send_pending_group_update(
        &self,
        festival_id: &str,
        group_id: &str,
        envelope: &offbeat_core::proto::GossipEnvelope,
    ) -> bool {
        let relay_matches = self
            .relay_festival_id
            .read()
            .is_ok_and(|active| active.as_deref() == Some(festival_id));
        if !relay_matches {
            return false;
        }
        let Some(relay) = self.inner.ws_relay.read().clone() else {
            return false;
        };
        let doc_id = format!("group/{group_id}/state");
        match relay.send_gossip_confirmed(&doc_id, envelope).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "group update remains queued for relay retry");
                false
            }
        }
    }

    async fn publish_group_state_update(
        &self,
        group_id: &str,
        group_key: [u8; 32],
        encrypted_update: Vec<u8>,
    ) -> anyhow::Result<()> {
        use offbeat_core::gossip_manager::GossipMessage;
        use offbeat_core::proto::GossipEnvelope;

        let publish_lock = self.group_publish_lock(group_id);
        let _publish_guard = publish_lock.lock().await;
        let festival_id = self
            .inner
            .db
            .load_group_festival_id(group_id)?
            .filter(|festival_id| !festival_id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("group has no verified festival scope"))?;
        let envelope = GossipEnvelope::from_gossip_message(&GossipMessage::GroupUpdate {
            doc_id: format!("group/{group_id}/state"),
            encrypted: encrypted_update.clone(),
            group_key,
        });
        let pending_id = self.inner.db.enqueue_group_update(
            &festival_id,
            group_id,
            &offbeat_core::proto::encode_envelope(&envelope),
        )?;
        self.inner
            .notifier
            .notify_doc(&format!("group/{group_id}/state"));

        if self
            .send_group_state_update(&festival_id, group_id, group_key, encrypted_update)
            .await
        {
            self.inner.db.delete_pending_group_update(pending_id)?;
        } else {
            self.schedule_pending_group_retry(offbeat_core::db::PendingGroupUpdate {
                id: pending_id,
                festival_id,
                group_id: group_id.to_string(),
                envelope: offbeat_core::proto::encode_envelope(&envelope),
            });
        }
        Ok(())
    }

    async fn flush_pending_group_updates(&self, festival_id: &str) -> anyhow::Result<()> {
        for pending in self.inner.db.load_pending_group_updates(festival_id)? {
            let publish_lock = self.group_publish_lock(&pending.group_id);
            let _publish_guard = publish_lock.lock().await;
            if !self.inner.db.pending_group_update_exists(pending.id)? {
                continue;
            }
            let envelope = match offbeat_core::proto::decode_envelope(&pending.envelope) {
                Ok(envelope) => envelope,
                Err(_) => {
                    tracing::warn!("discarding malformed queued group envelope");
                    self.inner.db.delete_pending_group_update(pending.id)?;
                    continue;
                }
            };
            if self
                .send_pending_group_update(festival_id, &pending.group_id, &envelope)
                .await
            {
                self.inner.db.delete_pending_group_update(pending.id)?;
            } else {
                self.schedule_pending_group_retry(pending);
            }
        }
        Ok(())
    }

    fn schedule_pending_group_retry(&self, pending: offbeat_core::db::PendingGroupUpdate) {
        let mut retries = self.pending_group_retries.lock().unwrap();
        if !retries.insert(pending.id) {
            return;
        }
        drop(retries);

        let generation = *self
            .group_retry_generations
            .lock()
            .unwrap()
            .entry(pending.group_id.clone())
            .or_default();
        let db = Arc::clone(&self.inner.db);
        let ws_relay = Arc::clone(&self.inner.ws_relay);
        let relay_festival_id = Arc::clone(&self.relay_festival_id);
        let pending_group_retries = Arc::clone(&self.pending_group_retries);
        let group_retry_generations = Arc::clone(&self.group_retry_generations);
        let publish_lock = self.group_publish_lock(&pending.group_id);
        self.spawn_background_task(async move {
            let Ok(envelope) = offbeat_core::proto::decode_envelope(&pending.envelope) else {
                let _ = db.delete_pending_group_update(pending.id);
                pending_group_retries.lock().unwrap().remove(&pending.id);
                return;
            };
            let doc_id = format!("group/{}/state", pending.group_id);
            let mut delay = std::time::Duration::from_secs(1);

            loop {
                tokio::time::sleep(delay).await;
                let _publish_guard = publish_lock.lock().await;
                let is_current_generation = group_retry_generations
                    .lock()
                    .unwrap()
                    .get(&pending.group_id)
                    .is_some_and(|current| *current == generation);
                let row_exists = db.pending_group_update_exists(pending.id).unwrap_or(false);
                if !is_current_generation || !row_exists {
                    pending_group_retries.lock().unwrap().remove(&pending.id);
                    break;
                }
                let relay_matches = relay_festival_id
                    .read()
                    .is_ok_and(|active| active.as_deref() == Some(pending.festival_id.as_str()));
                let relay = relay_matches.then(|| ws_relay.read().clone()).flatten();
                if let Some(relay) = relay
                    && relay
                        .send_gossip_confirmed(&doc_id, &envelope)
                        .await
                        .is_ok()
                {
                    let _ = db.delete_pending_group_update(pending.id);
                    pending_group_retries.lock().unwrap().remove(&pending.id);
                    break;
                }
                delay = (delay * 2).min(std::time::Duration::from_secs(30));
            }
        });
    }

    async fn deregister_group_resources(
        &self,
        festival_id: &str,
        group_id: &str,
        group_key: [u8; 32],
    ) {
        let state_doc_id = format!("group/{group_id}/state");
        self.inner.notifier.unwatch_doc(&state_doc_id);
        if let Ok(mut registry) = self.inner.resource_registry.write() {
            registry.deregister_group(group_key);
        }
        self.inner.sync_orchestrator.evict_group_key(group_id);

        if let Some(gossip) = &self.inner.gossip_manager {
            let mut gossip = gossip.lock().await;
            gossip.unsubscribe(offbeat_core::topics::group_topic(&group_key, "state"));
            gossip.unsubscribe(offbeat_core::topics::group_topic(&group_key, "chat"));
        }
        if self.relay_matches_festival(festival_id)
            && let Some(relay) = { self.inner.ws_relay.read().clone() }
        {
            let _ = relay
                .unsubscribe(vec![
                    format!("group/{group_id}/state"),
                    format!("group/{group_id}/chat"),
                ])
                .await;
        }
    }

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

        self.register_group_resources(&result.group_id, result.group_key)?;
        self.publish_group_state_update(
            &result.group_id,
            result.group_key,
            result.encrypted_update,
        )
        .await?;
        if let Err(error) = self.reconcile_shared_stars(&festival_id).await {
            tracing::warn!(%festival_id, %error, "group created; schedule reconciliation will retry");
        }

        // Network catch-up cannot roll back a locally completed create.
        if self.relay_matches_festival(&festival_id)
            && let Some(ws) = { self.inner.ws_relay.read().clone() }
            && let Err(error) = self
                .inner
                .sync_orchestrator
                .sync_with_peer_for_festival(ws.as_ref(), &festival_id)
                .await
        {
            tracing::warn!(%error, "group created; relay catch-up will retry");
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
        festival_id: String,
        display_name: String,
    ) -> anyhow::Result<GroupJoinResultDto> {
        use offbeat_core::auth;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);

        let result = self
            .inner
            .group_manager
            .join_group_for_festival(&invite_payload, &festival_id, &user_id, &display_name)
            .await?;

        self.register_group_resources(&result.group_id, result.group_key)?;
        self.publish_group_state_update(
            &result.group_id,
            result.group_key,
            result.encrypted_update,
        )
        .await?;
        if let Err(error) = self.reconcile_shared_stars(&result.festival_id).await {
            tracing::warn!(
                festival_id = %result.festival_id,
                %error,
                "group joined; schedule reconciliation will retry"
            );
        }

        // Network catch-up cannot roll back a locally completed join.
        if self.relay_matches_festival(&festival_id)
            && let Some(ws) = { self.inner.ws_relay.read().clone() }
            && let Err(error) = self
                .inner
                .sync_orchestrator
                .sync_with_peer_for_festival(ws.as_ref(), &festival_id)
                .await
        {
            tracing::warn!(%error, "group joined; relay catch-up will retry");
        }

        Ok(GroupJoinResultDto {
            group_id: result.group_id,
            festival_id: result.festival_id,
        })
    }

    /// Leave a group.
    pub async fn leave_group(&self, group_id: String) -> anyhow::Result<()> {
        use offbeat_core::auth;
        use offbeat_core::gossip_manager::GossipMessage;
        use offbeat_core::proto::GossipEnvelope;

        let festival_id = self
            .inner
            .db
            .load_group_festival_id(&group_id)?
            .filter(|festival_id| !festival_id.is_empty())
            .ok_or_else(|| anyhow::anyhow!("group has no verified festival scope"))?;
        let publish_lock = self.group_publish_lock(&group_id);
        let _publish_guard = publish_lock.lock().await;
        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let result = self
            .inner
            .group_manager
            .leave_group(&group_id, &user_id)
            .await?;
        let envelope = GossipEnvelope::from_gossip_message(&GossipMessage::GroupUpdate {
            doc_id: format!("group/{group_id}/state"),
            encrypted: result.encrypted_update.clone(),
            group_key: result.group_key,
        });

        // Invalidate every older retry before atomically replacing its rows
        // with the leave tombstone and purging local private state.
        {
            let mut generations = self.group_retry_generations.lock().unwrap();
            *generations.entry(group_id.clone()).or_default() += 1;
        }
        let pending_id = self.inner.db.finalize_group_leave(
            &festival_id,
            &group_id,
            &offbeat_core::proto::encode_envelope(&envelope),
        )?;
        self.inner
            .doc_manager
            .remove(&format!("group/{group_id}/state"))?;
        self.inner
            .notifier
            .unwatch_chat(&format!("group/{group_id}/chat"));
        self.deregister_group_resources(&festival_id, &group_id, result.group_key)
            .await;

        if self
            .send_group_state_update(
                &festival_id,
                &group_id,
                result.group_key,
                result.encrypted_update,
            )
            .await
        {
            self.inner.db.delete_pending_group_update(pending_id)?;
        } else {
            self.schedule_pending_group_retry(offbeat_core::db::PendingGroupUpdate {
                id: pending_id,
                festival_id,
                group_id,
                envelope: offbeat_core::proto::encode_envelope(&envelope),
            });
        }
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

        let group_key = self
            .inner
            .db
            .load_group_key(&group_id)?
            .ok_or_else(|| anyhow::anyhow!("group key not found for {group_id}"))?;
        self.publish_group_state_update(&group_id, group_key, encrypted)
            .await?;

        if let Some(stage_id) = stage_id
            && let Ok(Some(festival_id)) = self.inner.db.load_group_festival_id(&group_id)
            && let Err(error) = self
                .subscribe_chat_topics(festival_id, vec![stage_id])
                .await
        {
            tracing::warn!(%error, "check-in stage chat subscription failed");
        }
        Ok(())
    }

    /// Return the locally owned app-wide check-in for a festival.
    pub fn get_festival_check_in(
        &self,
        festival_id: String,
    ) -> anyhow::Result<Option<FestivalCheckInDto>> {
        let Some(checkin) = self.inner.db.load_festival_checkin(&festival_id)? else {
            return Ok(None);
        };
        Ok(Some(FestivalCheckInDto {
            festival_id: checkin.festival_id,
            kind: checkin.kind,
            value: checkin.value,
            checked_at: checkin.checked_at,
            expires_at: checkin.expires_at,
            revision: checkin.revision,
            pending_group_count: 0,
        }))
    }

    /// Persist one festival check-in and fan it out to every joined group.
    pub async fn set_festival_check_in(
        &self,
        festival_id: String,
        kind: String,
        value: Option<String>,
    ) -> anyhow::Result<FestivalCheckInDto> {
        use offbeat_core::auth;
        let normalized_value = value.map(|value| value.trim().to_string());
        let (stage_id, custom_location) = match kind.as_str() {
            "stage" => {
                let value = normalized_value
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("stage check-in requires a stage ID"))?;
                (Some(value), None)
            }
            "campsite" => (None, Some("Campsite")),
            "custom" => {
                let value = normalized_value
                    .as_deref()
                    .filter(|value| !value.is_empty() && value.len() <= 80)
                    .ok_or_else(|| anyhow::anyhow!("custom location must be 1-80 characters"))?;
                if value.chars().any(char::is_control) {
                    anyhow::bail!("custom location contains control characters");
                }
                (None, Some(value))
            }
            "none" => (None, None),
            _ => anyhow::bail!("unknown check-in kind"),
        };

        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        )?;
        let revision = self
            .inner
            .db
            .load_festival_checkin(&festival_id)?
            .map_or(1, |checkin| checkin.revision + 1);
        if kind == "none" {
            self.inner.db.clear_festival_checkin(&festival_id)?;
        } else {
            self.inner
                .db
                .save_festival_checkin(&offbeat_core::db::FestivalCheckIn {
                    festival_id: festival_id.clone(),
                    kind: kind.clone(),
                    value: normalized_value.clone(),
                    checked_at: now,
                    expires_at: now + offbeat_core::groups::CHECK_IN_FRESHNESS_SECS as i64,
                    revision,
                })?;
        }

        let signing_key = auth::generate_or_load_identity(&self.inner.db)?;
        let user_id = auth::get_user_id(&signing_key);
        let groups = self.inner.db.load_groups(&festival_id)?;
        let mut pending_group_count = 0u32;
        for (group_id, _, _) in groups {
            let encrypted = self
                .inner
                .group_manager
                .check_in(&group_id, &user_id, stage_id, custom_location)
                .await?;
            let group_key = self
                .inner
                .db
                .load_group_key(&group_id)?
                .ok_or_else(|| anyhow::anyhow!("group key not found for {group_id}"))?;
            if self
                .publish_group_state_update(&group_id, group_key, encrypted)
                .await
                .is_err()
            {
                pending_group_count += 1;
            }
        }
        if let Some(stage_id) = stage_id {
            let _ = self
                .subscribe_chat_topics(festival_id.clone(), vec![stage_id.to_string()])
                .await;
        }

        Ok(FestivalCheckInDto {
            festival_id,
            kind,
            value: normalized_value,
            checked_at: now,
            expires_at: now + offbeat_core::groups::CHECK_IN_FRESHNESS_SECS as i64,
            revision,
            pending_group_count,
        })
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

        let group_key = self
            .inner
            .db
            .load_group_key(&group_id)?
            .ok_or_else(|| anyhow::anyhow!("group key not found for {group_id}"))?;
        self.publish_group_state_update(&group_id, group_key, encrypted)
            .await?;
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

        let group_key = self
            .inner
            .db
            .load_group_key(&group_id)?
            .ok_or_else(|| anyhow::anyhow!("group key not found for {group_id}"))?;
        self.publish_group_state_update(&group_id, group_key, encrypted)
            .await?;
        Ok(())
    }

    /// Read the current state of a group from the local Yrs doc.
    pub async fn get_group_state(&self, group_id: String) -> anyhow::Result<GroupStateDto> {
        if self.inner.db.load_group_key(&group_id)?.is_none() {
            anyhow::bail!("group membership not found");
        }
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
                    location_kind: m.location_kind,
                    stage_id: m.stage_id,
                    custom_location: m.custom_location,
                    updated_at: m.updated_at,
                    expires_at: m.expires_at,
                    starred_set_ids: m.starred_set_ids,
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

        self.spawn_background_task(async move {
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

        self.spawn_background_task(async move {
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
        if self.inner.db.load_group_key(&group_id)?.is_none() {
            anyhow::bail!("group membership not found");
        }
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let doc_id = format!("group/{group_id}/state");
        let group_manager = Arc::clone(&self.inner.group_manager);
        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_doc(&doc_id);
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);
        let group_id_clone = group_id.clone();

        self.spawn_background_task(async move {
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
                            location_kind: m.location_kind,
                            stage_id: m.stage_id,
                            custom_location: m.custom_location,
                            updated_at: m.updated_at,
                            expires_at: m.expires_at,
                            starred_set_ids: m.starred_set_ids,
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
                                location_kind: m.location_kind,
                                stage_id: m.stage_id,
                                custom_location: m.custom_location,
                                updated_at: m.updated_at,
                                expires_at: m.expires_at,
                                starred_set_ids: m.starred_set_ids,
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
        self.ensure_group_chat_access(&topic)?;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let db = Arc::clone(&self.inner.db);
        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_chat(&topic);
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);
        let topic_clone = topic.clone();

        self.spawn_background_task(async move {
            // Emit initial messages
            if let Ok(msgs) = db.get_recent_chat_messages(&topic_clone, last_n, 0) {
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
                        trust: chat_trust_label(m.trust),
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
                if let Ok(msgs) = db.get_recent_chat_messages(&topic_clone, last_n, 0) {
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
                            trust: chat_trust_label(m.trust),
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
            ble,
            gm,
            cm,
            so,
            endpoint,
            doc_manager,
            self.inner.ble_child_tasks.clone(),
        ));
        self.ble_task_handles = handles;
        tracing::info!("subscription manager + BLE auto-connection tasks started");
    }

    /// Stop the BLE background tasks and wait until their captured state drops.
    pub async fn stop_ble_sync(&mut self) {
        let handles = self.ble_task_handles.drain(..).collect::<Vec<_>>();
        for handle in &handles {
            handle.abort();
        }
        for handle in handles {
            let _ = handle.await;
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
        self.stop_ble_sync().await;
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
    pub fn watch_peer_list(&self, sink: StreamSink<Vec<PeerStatusInfo>>) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let cm = self.inner.connection_manager.clone();
        let sink = Arc::new(Mutex::new(sink));

        self.spawn_background_task(async move {
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

        self.spawn_background_task(async move {
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
    pub fn watch_sync_status(&self, sink: StreamSink<SyncStatusDto>) -> anyhow::Result<()> {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let notifier = Arc::clone(&self.inner.notifier);

        let mut rx = notifier.watch_sync_status();
        let sink = Arc::new(Mutex::new(sink));
        let sink_clone = Arc::clone(&sink);

        self.spawn_background_task(async move {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logout_replaces_private_runtime_state_and_keeps_public_documents() {
        RUNTIME.block_on(async {
            let mut node = AppNode::create_in_memory().unwrap();
            node.inner
                .db
                .save_doc("festival/f1/state", "festival", b"public")
                .unwrap();
            node.inner
                .db
                .save_doc("group/g1/state", "group", b"private")
                .unwrap();
            node.inner
                .db
                .save_group("g1", "f1", "Friends", &[3; 32])
                .unwrap();
            let identity =
                offbeat_core::auth::derive_identity_from_prf(&node.inner.db, &[4; 32]).unwrap();
            let issuer = offbeat_core::signing::generate_signing_key();
            let issuer_key = issuer.verifying_key().to_bytes();
            let issuer_hex = issuer_key
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            offbeat_core::auth::pin_main_do_public_key(&node.inner.db, &issuer_hex).unwrap();
            let public_key = offbeat_core::auth::get_public_key_hex(&identity);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let message = format!(
                "attestation:v1:{public_key}:{now}:{}",
                now + 30 * 86400
            );
            let signature = offbeat_core::signing::sign(&issuer, message.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            offbeat_core::auth::store_attestation(
                &node.inner.db,
                &offbeat_core::auth::Attestation {
                    message,
                    signature,
                    issuer: issuer_hex,
                },
            )
            .unwrap();
            node.register_group_resources("g1", [3; 32]).unwrap();
            let group_resource_id = offbeat_core::resource::Resource::group_state([3; 32]).id();
            assert!(node
                .inner
                .resource_registry
                .read()
                .unwrap()
                .get(&group_resource_id)
                .is_some());

            node.logout_preserving_public_data().await.unwrap();

            assert_eq!(
                node.inner.db.load_doc("festival/f1/state").unwrap(),
                Some(b"public".to_vec())
            );
            assert!(node
                .inner
                .db
                .load_doc("group/g1/state")
                .unwrap()
                .is_none());
            assert!(node
                .inner
                .db
                .get_credential("identity_secret_key")
                .unwrap()
                .is_none());
            assert!(node
                .inner
                .resource_registry
                .read()
                .unwrap()
                .get(&group_resource_id)
                .is_none());
            assert!(node.inner.db.load_all_group_keys().unwrap().is_empty());
        });
    }

    #[test]
    fn logout_without_offline_recovery_proof_keeps_the_account_active() {
        RUNTIME.block_on(async {
            let mut node = AppNode::create_in_memory().unwrap();
            offbeat_core::auth::derive_identity_from_prf(&node.inner.db, &[9; 32]).unwrap();

            assert!(node.logout_preserving_public_data().await.is_err());
            assert!(node
                .inner
                .db
                .get_credential("identity_secret_key")
                .unwrap()
                .is_some());
        });
    }

    #[test]
    fn festival_registry_bridge_persists_normalized_snapshot_without_networking() {
        let path =
            std::env::temp_dir().join(format!("offbeat-registry-{}.db", uuid::Uuid::new_v4()));
        let store = FestivalRegistryCacheStore::open(path.to_string_lossy().into_owned()).unwrap();
        let payload = serde_json::json!([{
            "id": "fieldday",
            "name": "Field Day",
            "year": 2027,
            "location": "Brockwell Park",
            "city": "London",
            "country": "GB",
            "startDate": "2027-05-29",
            "endDate": "2027-05-29",
            "stages": [{
                "id": "main",
                "name": "Main Stage",
                "short": "MAIN",
                "color": "#ff2d8f",
                "order": 0
            }],
            "genres": ["electronic"],
            "status": "upcoming",
            "clashfinderId": "fieldday",
            "publicKey": "",
            "updatedAt": "2027-01-01T00:00:00Z",
            "lat": 51.45,
            "lon": -0.11
        }])
        .to_string();

        store
            .replace(
                payload,
                "2027-01-02T00:00:00Z".to_string(),
                "00000000000000000001".to_string(),
            )
            .unwrap();
        drop(store);
        let reopened =
            FestivalRegistryCacheStore::open(path.to_string_lossy().into_owned()).unwrap();
        let cached = reopened.load().unwrap().unwrap();
        let decoded: serde_json::Value = serde_json::from_str(&cached.payload_json).unwrap();
        assert_eq!(decoded[0]["id"], "fieldday");
        assert_eq!(decoded[0]["stages"][0]["name"], "Main Stage");
        assert_eq!(cached.fetched_at, "2027-01-02T00:00:00Z");
        assert_eq!(cached.request_token, "00000000000000000001");
        drop(reopened);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
    }

    #[test]
    fn festival_subscription_registers_resource_without_relay() {
        RUNTIME.block_on(async {
            let mut node = AppNode::create_in_memory().unwrap();
            node.set_festival_public_key("fest-1".to_string(), "11".repeat(32))
                .unwrap();

            node.subscribe_festival("fest-1".to_string())
                .await
                .unwrap();

            let registry = node.inner.resource_registry.read().unwrap();
            let resource = offbeat_core::resource::Resource::festival_state(
                "fest-1",
                [0x11; 32],
            );
            assert!(
                registry.get(&resource.id()).is_some(),
                "festival state must be registered before any relay connects"
            );
        });
    }

    #[test]
    fn group_lifecycle_registers_notifies_and_deregisters_resources() {
        RUNTIME.block_on(async {
            let creator = AppNode::create_in_memory().unwrap();
            let created = creator
                .create_group(
                    "fest-1".to_string(),
                    "The Crew".to_string(),
                    "Alice".to_string(),
                )
                .await
                .unwrap();
            let group_key = creator
                .inner
                .db
                .load_group_key(&created.group_id)
                .unwrap()
                .unwrap();
            {
                let registry = creator.inner.resource_registry.read().unwrap();
                assert!(
                    registry
                        .get(&offbeat_core::resource::Resource::group_state(group_key).id())
                        .is_some()
                );
                assert!(
                    registry
                        .get(&offbeat_core::resource::Resource::group_chat(group_key).id())
                        .is_some()
                );
            }

            let rejected = AppNode::create_in_memory().unwrap();
            assert!(
                rejected
                    .join_group(
                        created.invite_payload.clone(),
                        "other-festival".to_string(),
                        "Mallory".to_string(),
                    )
                    .await
                    .is_err()
            );
            assert!(
                rejected
                    .inner
                    .db
                    .load_groups("other-festival")
                    .unwrap()
                    .is_empty()
            );

            let mut joiner = AppNode::create_in_memory().unwrap();
            joiner
                .toggle_star("fest-1".to_string(), "set-a".to_string())
                .await
                .unwrap();
            let joined = joiner
                .join_group(
                    created.invite_payload,
                    "fest-1".to_string(),
                    "Bob".to_string(),
                )
                .await
                .unwrap();
            let joiner_key = joiner
                .inner
                .db
                .load_group_key(&joined.group_id)
                .unwrap()
                .unwrap();
            *joiner.relay_festival_id.write().unwrap() = Some("other-festival".to_string());
            assert!(
                !joiner.relay_matches_festival("fest-1"),
                "group unsubscribe must not use another festival's relay"
            );
            assert!(
                joiner
                    .get_invite_payload(joined.group_id.clone(), "other-festival".to_string())
                    .is_err(),
                "invite reconstruction must use the stored festival scope"
            );
            {
                let registry = joiner.inner.resource_registry.read().unwrap();
                assert!(
                    registry
                        .get(&offbeat_core::resource::Resource::group_state(joiner_key).id())
                        .is_some()
                );
                assert!(
                    registry
                        .get(&offbeat_core::resource::Resource::group_chat(joiner_key).id())
                        .is_some()
                );
            }

            let doc_id = format!("group/{}/state", joined.group_id);
            let mut state_rx = joiner.inner.notifier.watch_doc(&doc_id);
            joiner
                .check_in(joined.group_id.clone(), Some("main".to_string()), None)
                .await
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(1), state_rx.changed())
                .await
                .expect("local group watcher should be notified")
                .unwrap();
            joiner
                .toggle_star("fest-1".to_string(), "set-b".to_string())
                .await
                .unwrap();
            let state = joiner
                .get_group_state(joined.group_id.clone())
                .await
                .unwrap();
            assert_eq!(state.members.len(), 1);
            assert_eq!(state.members[0].starred_set_ids, vec!["set-a", "set-b"]);
            assert_eq!(state.members[0].stage_id.as_deref(), Some("main"));
            assert!(state.members[0].custom_location.is_none());

            joiner
                .toggle_star("fest-1".to_string(), "set-a".to_string())
                .await
                .unwrap();
            let state = joiner
                .get_group_state(joined.group_id.clone())
                .await
                .unwrap();
            assert_eq!(state.members[0].starred_set_ids, vec!["set-b"]);

            // Startup subscription reconciles any personal mutation that was
            // persisted before its group document could be updated.
            joiner.inner.db.toggle_star("fest-1", "set-c").unwrap();
            joiner.subscribe_groups("fest-1".to_string()).await.unwrap();
            let state = joiner
                .get_group_state(joined.group_id.clone())
                .await
                .unwrap();
            assert_eq!(state.members[0].starred_set_ids, vec!["set-b", "set-c"]);

            let chat_topic = format!("group/{}/chat", joined.group_id);
            joiner
                .inner
                .db
                .save_chat_message(&offbeat_core::types::ChatMessage {
                    id: "private-message".to_string(),
                    user_id: "user".to_string(),
                    display_name: "User".to_string(),
                    text: "secret".to_string(),
                    topic: chat_topic.clone(),
                    stage_id: None,
                    timestamp: "2026-01-01T00:00:00Z".to_string(),
                    writer_seq: 1,
                    logical_time: 1,
                    writer_key: Vec::new(),
                    signature: Vec::new(),
                    trust: offbeat_core::types::ChatTrust::Unverified,
                })
                .unwrap();
            assert_eq!(
                joiner
                    .get_chat_history(chat_topic.clone(), 10, 0)
                    .unwrap()
                    .len(),
                1
            );

            joiner.leave_group(joined.group_id.clone()).await.unwrap();
            let pending_leave = joiner
                .inner
                .db
                .load_pending_group_updates("fest-1")
                .unwrap();
            assert_eq!(
                pending_leave.len(),
                1,
                "leave compacts every older group delta into one tombstone"
            );
            let leave_envelope =
                offbeat_core::proto::decode_envelope(&pending_leave[0].envelope).unwrap();
            assert!(matches!(
                leave_envelope.payload,
                Some(offbeat_core::proto::gossip_envelope::Payload::GroupUpdate(
                    _
                ))
            ));
            assert!(
                joiner
                    .get_group_state(joined.group_id.clone())
                    .await
                    .is_err()
            );
            assert!(joiner.inner.db.load_doc(&doc_id).unwrap().is_none());
            assert!(
                joiner
                    .inner
                    .db
                    .get_chat_messages(&chat_topic, 10, 0)
                    .unwrap()
                    .is_empty()
            );
            assert!(joiner.get_chat_history(chat_topic, 10, 0).is_err());
            assert!(
                joiner
                    .inner
                    .db
                    .load_group_key(&joined.group_id)
                    .unwrap()
                    .is_none()
            );
            let registry = joiner.inner.resource_registry.read().unwrap();
            assert!(
                registry
                    .get(&offbeat_core::resource::Resource::group_state(joiner_key).id())
                    .is_none()
            );
            assert!(
                registry
                    .get(&offbeat_core::resource::Resource::group_chat(joiner_key).id())
                    .is_none()
            );
        });
    }
}
