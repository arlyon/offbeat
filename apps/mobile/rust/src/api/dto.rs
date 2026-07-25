// These types are re-exported from the parent module (mod.rs) and used
// in function signatures throughout this module.
use offbeat_core::OffbeatNode;
use offbeat_core::connection_manager::{GossipStatus, PeerEntry, PeerSource};
use offbeat_core::doc_manager::DocManager;
use offbeat_core::notifier::SyncStatus;

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
    pub festival_id: String,
    pub invite_payload: String,
}

pub struct GroupJoinResultDto {
    pub group_id: String,
    pub festival_id: String,
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
    pub year: i32,
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

pub struct HourlyWeatherDto {
    pub time: Vec<String>,
    pub temperature_2m: Vec<f64>,
    pub precipitation_probability: Vec<f64>,
    pub weather_code: Vec<u32>,
    pub wind_speed_10m: Vec<f64>,
}

pub struct WeatherForecastDto {
    pub updated_at: String,
    pub lat: f64,
    pub lon: f64,
    pub timezone: String,
    pub hourly: HourlyWeatherDto,
}

/// Per-resource sync status.
pub struct ResourceSyncStatusDto {
    pub id: String,
    pub syncing: bool,
    pub last_synced: Option<String>,
    pub error: Option<String>,
    pub messages_received: u32,
    pub messages_sent: u32,
    /// Number of peers subscribed to this topic on the relay.
    pub peer_count: u32,
}

/// Overall sync status for the node.
pub struct SyncStatusDto {
    pub syncing: bool,
    pub resources: Vec<ResourceSyncStatusDto>,
    pub pending_ops: u32,
}

// ---------------------------------------------------------------------------
// Transport DTOs
// ---------------------------------------------------------------------------

pub struct TransportStatusDto {
    /// Relay (Festival DO WebSocket) connection status.
    pub relay: RelayStatusDto,
    /// BLE transport status.
    pub ble: BleStatusDto,
}

pub struct RelayStatusDto {
    pub connected: bool,
    pub authenticated: bool,
    /// Bytes per second sent to the relay (computed over last interval).
    pub tx_bytes_per_sec: u64,
    /// Bytes per second received from the relay.
    pub rx_bytes_per_sec: u64,
}

pub struct BleStatusDto {
    pub active: bool,
    pub peer_count: u32,
    /// Aggregate BLE bytes per second sent.
    pub tx_bytes_per_sec: u64,
    /// Aggregate BLE bytes per second received.
    pub rx_bytes_per_sec: u64,
    pub retransmits: u64,
    pub peers: Vec<TransportPeerDto>,
    /// The list of UUIDs we are currently advertising (Discovery Beacons).
    pub advertising_beacons: Vec<String>,
}

pub struct TransportPeerDto {
    pub device_id: String,
    pub phase: String,
    pub connect_path: Option<String>,
    pub verified_endpoint: Option<String>,
    pub consecutive_failures: u32,
    /// The 12-byte prefix seen in the peer's advertisement.
    pub key_prefix: Option<String>,
}

// ---------------------------------------------------------------------------
// Meshtastic debug harness DTOs
// ---------------------------------------------------------------------------

pub struct MeshtasticDebugDeviceDto {
    pub device_id: String,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    pub services: Vec<String>,
}

pub struct MeshtasticDebugFrameDto {
    pub kind: String,
    pub topic_tag_hex: String,
    pub message_id_hex: String,
    pub body_hex: String,
    pub body_text: Option<String>,
}

pub struct MeshtasticDebugReportDto {
    pub device_id: String,
    pub connected: bool,
    pub mtu: u16,
    pub services: Vec<String>,
    pub sent_fragments: u32,
    pub raw_from_radio_count: u32,
    pub private_app_count: u32,
    pub applied_group_chats: u32,
    pub received_frames: Vec<MeshtasticDebugFrameDto>,
    pub events: Vec<String>,
}

// ---------------------------------------------------------------------------
// Peer / Connection Manager DTOs
// ---------------------------------------------------------------------------

/// A simplified view of a tracked peer for the Flutter UI.
pub struct PeerStatusInfo {
    pub endpoint_id: String,
    /// Discovery source: "crdt", "ble", or "gossip".
    pub source: String,
    /// Gossip connection status: "unknown", "joining", "active", or "stale".
    pub status: String,
    pub ble_visible: bool,
    pub relay_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read lineup from a doc manager (used by watch_lineup).
///
/// Reads from top-level named maps `"stages"`, `"days"`, `"sets"` where each
/// entry is a nested YMap of fields (not JSON strings).
pub fn read_lineup_from_doc(dm: &DocManager, doc_id: &str) -> Option<LineupDto> {
    use offbeat_core::doc_manager::{any_bool, any_i32, any_str};

    let stages: Vec<LineupStageDto> = dm
        .read_nested_map_entries(doc_id, "stages")
        .into_iter()
        .filter_map(|(id, f)| {
            Some(LineupStageDto {
                id,
                name: any_str(&f, "name")?,
                short: any_str(&f, "short")?,
                color: any_str(&f, "color")?,
                order: any_i32(&f, "order")?,
            })
        })
        .collect();

    let days: Vec<LineupDayDto> = dm
        .read_nested_map_entries(doc_id, "days")
        .into_iter()
        .filter_map(|(id, f)| {
            Some(LineupDayDto {
                id,
                label: any_str(&f, "label")?,
                num: any_i32(&f, "num")?,
                month: any_str(&f, "month")?,
                year: any_i32(&f, "year").unwrap_or(0),
            })
        })
        .collect();

    let sets: Vec<LineupSetDto> = dm
        .read_nested_map_entries(doc_id, "sets")
        .into_iter()
        .filter_map(|(id, f)| {
            Some(LineupSetDto {
                id,
                day: any_str(&f, "day")?,
                stage: any_str(&f, "stage")?,
                artist: any_str(&f, "artist")?,
                start_min: any_i32(&f, "startMin")?,
                duration_min: any_i32(&f, "durationMin")?,
                genre: any_str(&f, "genre")?,
                cancelled: any_bool(&f, "cancelled").unwrap_or(false),
            })
        })
        .collect();

    if stages.is_empty() && days.is_empty() && sets.is_empty() {
        return None;
    }

    Some(LineupDto { stages, days, sets })
}

/// Read weather from a doc manager (used by get_weather / watch_weather).
///
/// Weather metadata is stored in a `"weather"` top-level map under a `"meta"`
/// entry. Hourly data is in a separate `"hourly"` top-level map keyed by time
/// string, each entry containing temp/precip/code/wind fields.
pub fn read_weather_from_doc(dm: &DocManager, doc_id: &str) -> Option<WeatherForecastDto> {
    use offbeat_core::doc_manager::{any_f64, any_i32, any_str};

    let weather_fields = dm.read_nested_map_entry(doc_id, "weather", "meta")?;

    let updated_at = any_str(&weather_fields, "updatedAt")?;
    let lat = any_f64(&weather_fields, "lat")?;
    let lon = any_f64(&weather_fields, "lon")?;
    let timezone = any_str(&weather_fields, "timezone")?;

    // Read hourly entries from "hourly" top-level map
    let hourly_entries = dm.read_nested_map_entries(doc_id, "hourly");

    // Sort by time key and reconstruct correlated arrays
    let mut entries: Vec<_> = hourly_entries
        .into_iter()
        .filter_map(|(time, f)| {
            Some((
                time,
                any_f64(&f, "temp")?,
                any_f64(&f, "precip")?,
                any_i32(&f, "code")? as u32,
                any_f64(&f, "wind")?,
            ))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut time = Vec::with_capacity(entries.len());
    let mut temperature_2m = Vec::with_capacity(entries.len());
    let mut precipitation_probability = Vec::with_capacity(entries.len());
    let mut weather_code = Vec::with_capacity(entries.len());
    let mut wind_speed_10m = Vec::with_capacity(entries.len());

    for (t, temp, precip, code, wind) in entries {
        time.push(t);
        temperature_2m.push(temp);
        precipitation_probability.push(precip);
        weather_code.push(code);
        wind_speed_10m.push(wind);
    }

    Some(WeatherForecastDto {
        updated_at,
        lat,
        lon,
        timezone,
        hourly: HourlyWeatherDto {
            time,
            temperature_2m,
            precipitation_probability,
            weather_code,
            wind_speed_10m,
        },
    })
}

/// Convert a `PeerEntry` from the connection manager into a DTO for Flutter.
pub fn peer_entry_to_dto(entry: PeerEntry) -> PeerStatusInfo {
    let source = match entry.source {
        PeerSource::Crdt => "crdt",
        PeerSource::Ble => "ble",
        PeerSource::Gossip => "gossip",
    };
    let status = match entry.gossip_status {
        GossipStatus::Unknown => "unknown",
        GossipStatus::Joining => "joining",
        GossipStatus::Active => "active",
        GossipStatus::Stale => "stale",
    };
    PeerStatusInfo {
        endpoint_id: entry.endpoint_id,
        source: source.to_string(),
        status: status.to_string(),
        ble_visible: entry.ble_prefix_match,
        relay_url: entry.relay_url,
    }
}

/// One-shot transport snapshot (no rate computation — rates are zero).
pub fn snapshot_transport(node: &OffbeatNode) -> TransportStatusDto {
    let relay = match &*node.ws_relay.read() {
        Some(ws) => RelayStatusDto {
            connected: ws.is_connected(),
            authenticated: ws.is_authenticated(),
            tx_bytes_per_sec: 0,
            rx_bytes_per_sec: 0,
        },
        None => RelayStatusDto {
            connected: false,
            authenticated: false,
            tx_bytes_per_sec: 0,
            rx_bytes_per_sec: 0,
        },
    };
    let ble = match &node.ble_transport {
        Some(ble) => {
            let peers: Vec<TransportPeerDto> = ble
                .snapshot_peers()
                .into_iter()
                .map(|p| TransportPeerDto {
                    device_id: p.device_id.to_string(),
                    phase: format!("{:?}", p.phase),
                    connect_path: p.connect_path.map(|c| format!("{c:?}")),
                    verified_endpoint: p.verified_endpoint.map(|e| e.to_string()),
                    consecutive_failures: p.consecutive_failures,
                    key_prefix: p.prefix.map(hex::encode),
                })
                .collect();
            BleStatusDto {
                active: true,
                peer_count: peers.len() as u32,
                tx_bytes_per_sec: 0,
                rx_bytes_per_sec: 0,
                retransmits: ble.metrics().retransmits,
                peers,
                advertising_beacons: ble
                    .advertising_beacons()
                    .iter()
                    .map(|u| u.to_string())
                    .collect(),
            }
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
    TransportStatusDto { relay, ble }
}

/// Convert SyncStatus from notifier to DTO.
pub fn convert_sync_status(status: &SyncStatus) -> SyncStatusDto {
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
                messages_received: r.messages_received,
                messages_sent: r.messages_sent,
                peer_count: r.peer_count,
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
