//! Meshtastic route adapter and packet protocol.
//!
//! Meshtastic is modeled as a constrained physical path for the shared Offbeat
//! sync protocol, not as a separate semantic event lane. The phone talks to a
//! paired Meshtastic device over Bluetooth, and that device carries Offbeat
//! packets in Meshtastic `PRIVATE_APP` payloads over LoRa.
//!
//! This module defines the Offbeat `PRIVATE_APP` payload and wraps it in the
//! official Meshtastic protobuf envelope via the non-GPL `meshtastic_protobufs`
//! crate. It intentionally does not link the GPL `meshtastic` crate; platform
//! Bluetooth bridges should write the encoded `ToRadio` bytes to the official
//! Meshtastic GATT characteristics.

use std::collections::{HashMap, HashSet, VecDeque};

use meshtastic_protobufs::meshtastic::{self, from_radio, mesh_packet, to_radio};
use prost::Message;

use super::profile::{SyncEncoding, SyncPayloadKind, SyncPriority, TransportProfile};

pub const MESHTASTIC_PROFILE: TransportProfile = TransportProfile::Constrained;
/// Meshtastic protobuf `PortNum::PRIVATE_APP`.
pub const MESHTASTIC_PRIVATE_APP_PORTNUM: u32 = meshtastic::PortNum::PrivateApp as u32;
pub const MESHTASTIC_BROADCAST_NODE: u32 = 0xffff_ffff;
/// Conservative safe app payload budget used by Meshtastic docs/firmware.
pub const SAFE_PRIVATE_APP_PAYLOAD_BYTES: usize = 228;
pub const FRAME_VERSION: u8 = 1;
pub const DEFAULT_TTL: u8 = 3;
pub const TOPIC_TAG_BYTES: usize = 8;
pub const MESSAGE_ID_BYTES: usize = 8;
pub const PACKET_HEADER_BYTES: usize = 27;
pub const MAX_FRAGMENTS: u8 = 4;
const MAGIC: [u8; 2] = *b"OB";
const GROUP_CHAT_TAG_CONTEXT: &[u8] = b"offbeat/meshtastic/group/chat/v1";
const COMPACT_GROUP_CHAT_VERSION: u8 = 2;
const LEGACY_COMPACT_GROUP_CHAT_VERSION: u8 = 1;

pub fn group_chat_topic_tag(group_key: &[u8; 32]) -> [u8; TOPIC_TAG_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(GROUP_CHAT_TAG_CONTEXT);
    hasher.update(group_key);
    let hash = hasher.finalize();
    let mut tag = [0u8; TOPIC_TAG_BYTES];
    tag.copy_from_slice(&hash.as_bytes()[..TOPIC_TAG_BYTES]);
    tag
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactGroupChat {
    pub message_uuid: [u8; 16],
    pub user_id: String,
    pub display_name: String,
    pub text: String,
    pub writer_seq: u64,
    pub logical_time: u64,
    pub timestamp_secs: u64,
}

impl CompactGroupChat {
    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        let user_id = bounded_string_bytes("user_id", &self.user_id, 64)?;
        let display_name = bounded_string_bytes("display_name", &self.display_name, 48)?;
        let text = bounded_string_bytes("text", &self.text, 96)?;

        let mut out = Vec::with_capacity(44 + user_id.len() + display_name.len() + text.len());
        out.push(COMPACT_GROUP_CHAT_VERSION);
        out.extend_from_slice(&self.message_uuid);
        out.extend_from_slice(&self.writer_seq.to_be_bytes());
        out.extend_from_slice(&self.logical_time.to_be_bytes());
        out.extend_from_slice(&self.timestamp_secs.to_be_bytes());
        out.push(user_id.len() as u8);
        out.push(display_name.len() as u8);
        out.push(text.len() as u8);
        out.extend_from_slice(user_id);
        out.extend_from_slice(display_name);
        out.extend_from_slice(text);
        Ok(out)
    }

    pub fn decode(raw: &[u8]) -> anyhow::Result<Self> {
        if raw.len() < 36 {
            anyhow::bail!("compact group chat too short");
        }
        let mut message_uuid = [0u8; 16];
        message_uuid.copy_from_slice(&raw[1..17]);
        let writer_seq = u64::from_be_bytes(raw[17..25].try_into()?);
        let (logical_time, timestamp_secs, lengths_at) = match raw[0] {
            LEGACY_COMPACT_GROUP_CHAT_VERSION => {
                (writer_seq, u64::from_be_bytes(raw[25..33].try_into()?), 33)
            }
            COMPACT_GROUP_CHAT_VERSION if raw.len() >= 44 => (
                u64::from_be_bytes(raw[25..33].try_into()?),
                u64::from_be_bytes(raw[33..41].try_into()?),
                41,
            ),
            version => anyhow::bail!("unsupported compact group chat version {version}"),
        };
        let user_len = raw[lengths_at] as usize;
        let display_len = raw[lengths_at + 1] as usize;
        let text_len = raw[lengths_at + 2] as usize;
        let payload_start = lengths_at + 3;
        let expected = payload_start + user_len + display_len + text_len;
        if raw.len() != expected {
            anyhow::bail!("compact group chat length mismatch");
        }
        let user_start = payload_start;
        let display_start = user_start + user_len;
        let text_start = display_start + display_len;
        Ok(Self {
            message_uuid,
            user_id: String::from_utf8(raw[user_start..display_start].to_vec())?,
            display_name: String::from_utf8(raw[display_start..text_start].to_vec())?,
            text: String::from_utf8(raw[text_start..].to_vec())?,
            writer_seq,
            logical_time,
            timestamp_secs,
        })
    }
}

fn bounded_string_bytes<'a>(name: &str, value: &'a str, max: usize) -> anyhow::Result<&'a [u8]> {
    let bytes = value.as_bytes();
    if bytes.len() > max {
        anyhow::bail!(
            "compact group chat {name} is {} bytes; max is {max}",
            bytes.len()
        );
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffbeatSyncFrame {
    pub version: u8,
    pub profile: TransportProfile,
    pub kind: SyncPayloadKind,
    /// Short keyed/hash tag for the Offbeat topic/resource. Never put raw group ids on LoRa.
    pub topic_tag: [u8; TOPIC_TAG_BYTES],
    pub message_id: [u8; MESSAGE_ID_BYTES],
    pub ttl: u8,
    /// Compact body for `kind`; already signed/encrypted by higher layers where required.
    pub body: Vec<u8>,
}

impl OffbeatSyncFrame {
    pub fn new(
        kind: SyncPayloadKind,
        topic_tag: [u8; TOPIC_TAG_BYTES],
        message_id: [u8; MESSAGE_ID_BYTES],
        ttl: u8,
        body: Vec<u8>,
    ) -> anyhow::Result<Self> {
        match MESHTASTIC_PROFILE.decide(kind).encoding {
            SyncEncoding::CompactFrame { max_bytes } if body.len() <= max_bytes => Ok(Self {
                version: FRAME_VERSION,
                profile: MESHTASTIC_PROFILE,
                kind,
                topic_tag,
                message_id,
                ttl,
                body,
            }),
            SyncEncoding::CompactFrame { max_bytes } => anyhow::bail!(
                "compact Meshtastic body is {} bytes; max is {max_bytes}",
                body.len()
            ),
            SyncEncoding::Suppressed => anyhow::bail!("{kind:?} is suppressed on Meshtastic"),
            _ => anyhow::bail!("{kind:?} does not use compact encoding on Meshtastic"),
        }
    }

    /// Encode this logical frame into one or more Meshtastic `PRIVATE_APP` payloads.
    pub fn encode_private_app_payloads(&self) -> anyhow::Result<Vec<Vec<u8>>> {
        self.encode_private_app_payloads_with_mtu(SAFE_PRIVATE_APP_PAYLOAD_BYTES)
    }

    fn encode_private_app_payloads_with_mtu(&self, mtu: usize) -> anyhow::Result<Vec<Vec<u8>>> {
        if mtu <= PACKET_HEADER_BYTES {
            anyhow::bail!("Meshtastic MTU {mtu} leaves no body room");
        }
        let chunk_bytes = mtu - PACKET_HEADER_BYTES;
        let total = self.body.len().div_ceil(chunk_bytes).max(1);
        if total > MAX_FRAGMENTS as usize {
            anyhow::bail!("Meshtastic frame needs {total} fragments; max is {MAX_FRAGMENTS}");
        }

        let mut packets = Vec::with_capacity(total);
        for index in 0..total {
            let start = index * chunk_bytes;
            let end = usize::min(start + chunk_bytes, self.body.len());
            let packet = MeshtasticPacket {
                version: self.version,
                kind: self.kind,
                priority: self.kind.priority(),
                topic_tag: self.topic_tag,
                message_id: self.message_id,
                ttl: self.ttl,
                fragment_index: index as u8,
                fragment_total: total as u8,
                fragment: self.body[start..end].to_vec(),
            };
            packets.push(packet.encode()?);
        }
        Ok(packets)
    }
}

/// Offbeat's Meshtastic `PRIVATE_APP` payload packet.
///
/// Binary layout, all multibyte integers big-endian:
///
/// ```text
/// magic[2] = "OB"
/// version: u8
/// flags: u8             // reserved, currently 0
/// kind: u8              // SyncPayloadKind
/// priority: u8          // SyncPriority, for radio queues
/// topic_tag[8]
/// message_id[8]
/// ttl: u8
/// fragment_index: u8
/// fragment_total: u8
/// fragment_len: u16
/// fragment bytes...
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshtasticPacket {
    pub version: u8,
    pub kind: SyncPayloadKind,
    pub priority: SyncPriority,
    pub topic_tag: [u8; TOPIC_TAG_BYTES],
    pub message_id: [u8; MESSAGE_ID_BYTES],
    pub ttl: u8,
    pub fragment_index: u8,
    pub fragment_total: u8,
    pub fragment: Vec<u8>,
}

pub fn encode_to_radio_private_app(
    private_app_payload: Vec<u8>,
    priority: SyncPriority,
    hop_limit: u8,
    want_ack: bool,
) -> anyhow::Result<Vec<u8>> {
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::PrivateApp as i32,
        payload: private_app_payload,
        want_response: false,
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        to: MESHTASTIC_BROADCAST_NODE,
        hop_limit: hop_limit as u32,
        want_ack,
        priority: mesh_priority(priority) as i32,
        payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    Ok(meshtastic::ToRadio {
        payload_variant: Some(to_radio::PayloadVariant::Packet(packet)),
    }
    .encode_to_vec())
}

pub fn decode_to_radio_private_app(raw: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
    let to_radio = meshtastic::ToRadio::decode(raw)?;
    let Some(to_radio::PayloadVariant::Packet(packet)) = to_radio.payload_variant else {
        return Ok(None);
    };
    private_app_payload_from_mesh_packet(packet)
}

pub fn decode_from_radio_private_app(raw: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
    let from_radio = meshtastic::FromRadio::decode(raw)?;
    let Some(from_radio::PayloadVariant::Packet(packet)) = from_radio.payload_variant else {
        return Ok(None);
    };
    private_app_payload_from_mesh_packet(packet)
}

#[cfg(test)]
fn encode_from_radio_private_app(private_app_payload: Vec<u8>) -> Vec<u8> {
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::PrivateApp as i32,
        payload: private_app_payload,
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    meshtastic::FromRadio {
        id: 1,
        payload_variant: Some(from_radio::PayloadVariant::Packet(packet)),
    }
    .encode_to_vec()
}

fn private_app_payload_from_mesh_packet(
    packet: meshtastic::MeshPacket,
) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(mesh_packet::PayloadVariant::Decoded(data)) = packet.payload_variant else {
        return Ok(None);
    };
    if data.portnum != meshtastic::PortNum::PrivateApp as i32 {
        return Ok(None);
    }
    Ok(Some(data.payload))
}

fn mesh_priority(priority: SyncPriority) -> mesh_packet::Priority {
    match priority {
        SyncPriority::P0Critical => mesh_packet::Priority::Alert,
        SyncPriority::P1High => mesh_packet::Priority::High,
        SyncPriority::P2Normal => mesh_packet::Priority::Reliable,
        SyncPriority::P3Idle => mesh_packet::Priority::Background,
    }
}

impl MeshtasticPacket {
    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        if self.version != FRAME_VERSION {
            anyhow::bail!("unsupported packet version {}", self.version);
        }
        if self.fragment_total == 0 || self.fragment_total > MAX_FRAGMENTS {
            anyhow::bail!("invalid fragment_total {}", self.fragment_total);
        }
        if self.fragment_index >= self.fragment_total {
            anyhow::bail!(
                "fragment_index {} out of {}",
                self.fragment_index,
                self.fragment_total
            );
        }
        if self.fragment.len() > u16::MAX as usize {
            anyhow::bail!("fragment too large");
        }
        if PACKET_HEADER_BYTES + self.fragment.len() > SAFE_PRIVATE_APP_PAYLOAD_BYTES {
            anyhow::bail!(
                "PRIVATE_APP packet is {} bytes; safe max is {SAFE_PRIVATE_APP_PAYLOAD_BYTES}",
                PACKET_HEADER_BYTES + self.fragment.len()
            );
        }

        let mut out = Vec::with_capacity(PACKET_HEADER_BYTES + self.fragment.len());
        out.extend_from_slice(&MAGIC);
        out.push(self.version);
        out.push(0); // flags, reserved
        out.push(kind_to_u8(self.kind));
        out.push(priority_to_u8(self.priority));
        out.extend_from_slice(&self.topic_tag);
        out.extend_from_slice(&self.message_id);
        out.push(self.ttl);
        out.push(self.fragment_index);
        out.push(self.fragment_total);
        out.extend_from_slice(&(self.fragment.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.fragment);
        Ok(out)
    }

    pub fn decode(raw: &[u8]) -> anyhow::Result<Self> {
        if raw.len() < PACKET_HEADER_BYTES {
            anyhow::bail!("PRIVATE_APP payload too short");
        }
        if raw[0..2] != MAGIC {
            anyhow::bail!("not an Offbeat Meshtastic packet");
        }
        let version = raw[2];
        if version != FRAME_VERSION {
            anyhow::bail!("unsupported packet version {version}");
        }
        let flags = raw[3];
        if flags != 0 {
            anyhow::bail!("unsupported packet flags {flags}");
        }
        let kind = kind_from_u8(raw[4])?;
        let priority = priority_from_u8(raw[5])?;
        let mut topic_tag = [0u8; TOPIC_TAG_BYTES];
        topic_tag.copy_from_slice(&raw[6..14]);
        let mut message_id = [0u8; MESSAGE_ID_BYTES];
        message_id.copy_from_slice(&raw[14..22]);
        let ttl = raw[22];
        let fragment_index = raw[23];
        let fragment_total = raw[24];
        if fragment_total == 0 || fragment_total > MAX_FRAGMENTS {
            anyhow::bail!("invalid fragment_total {fragment_total}");
        }
        if fragment_index >= fragment_total {
            anyhow::bail!("fragment_index {fragment_index} out of {fragment_total}");
        }
        let fragment_len = u16::from_be_bytes([raw[25], raw[26]]) as usize;
        if raw.len() != PACKET_HEADER_BYTES + fragment_len {
            anyhow::bail!("fragment length mismatch");
        }

        Ok(Self {
            version,
            kind,
            priority,
            topic_tag,
            message_id,
            ttl,
            fragment_index,
            fragment_total,
            fragment: raw[PACKET_HEADER_BYTES..].to_vec(),
        })
    }
}

/// Reassembles fragment packets into logical constrained sync frames.
#[derive(Default)]
pub struct MeshtasticReassembly {
    partial: HashMap<PacketKey, PartialFrame>,
}

impl MeshtasticReassembly {
    pub fn push(&mut self, packet: MeshtasticPacket) -> anyhow::Result<Option<OffbeatSyncFrame>> {
        if packet.priority != packet.kind.priority() {
            anyhow::bail!("packet priority does not match payload kind");
        }
        let key = PacketKey {
            topic_tag: packet.topic_tag,
            message_id: packet.message_id,
        };
        let entry = self.partial.entry(key).or_insert_with(|| PartialFrame {
            kind: packet.kind,
            ttl: packet.ttl,
            fragments: vec![None; packet.fragment_total as usize],
        });
        if entry.kind != packet.kind || entry.fragments.len() != packet.fragment_total as usize {
            anyhow::bail!("inconsistent Meshtastic fragments for message");
        }

        entry.fragments[packet.fragment_index as usize] = Some(packet.fragment);
        if entry.fragments.iter().any(Option::is_none) {
            return Ok(None);
        }

        let Some(entry) = self.partial.remove(&key) else {
            anyhow::bail!("complete Meshtastic fragment entry disappeared");
        };
        let mut body = Vec::new();
        for fragment in entry.fragments.into_iter().flatten() {
            body.extend_from_slice(&fragment);
        }
        Ok(Some(OffbeatSyncFrame::new(
            entry.kind,
            key.topic_tag,
            key.message_id,
            entry.ttl,
            body,
        )?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PacketKey {
    topic_tag: [u8; TOPIC_TAG_BYTES],
    message_id: [u8; MESSAGE_ID_BYTES],
}

struct PartialFrame {
    kind: SyncPayloadKind,
    ttl: u8,
    fragments: Vec<Option<Vec<u8>>>,
}

pub const MESHTASTIC_BLE_SERVICE_UUID: &str = "6ba1b218-15a8-461f-9fa8-5dcae273eafd";
pub const MESHTASTIC_TO_RADIO_CHAR_UUID: &str = "f75c76d2-129e-4dad-a1dd-7866124401e7";
pub const MESHTASTIC_FROM_RADIO_CHAR_UUID: &str = "2c55e69e-4993-11ed-b878-0242ac120002";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeshtasticDeviceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshtasticDevice {
    pub id: MeshtasticDeviceId,
    pub name: Option<String>,
    pub rssi: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshtasticConnectionState {
    Stopped,
    Scanning,
    Connecting { device_id: MeshtasticDeviceId },
    Connected { device_id: MeshtasticDeviceId },
    Backoff { attempt: u32 },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshtasticStatus {
    pub state: MeshtasticConnectionState,
    pub queued_frames: usize,
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub rx_frames: u64,
    pub dropped_duplicates: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshtasticSidecarConfig {
    /// Preferred bonded/paired Meshtastic device. If absent, `start` scans and
    /// connects to the first discovered Meshtastic BLE service.
    pub preferred_device: Option<MeshtasticDeviceId>,
    pub max_tx_queue: usize,
}

impl Default for MeshtasticSidecarConfig {
    fn default() -> Self {
        Self {
            preferred_device: None,
            max_tx_queue: 32,
        }
    }
}

/// Hardware-facing adapter. Production implementations talk to a paired
/// Meshtastic device over Bluetooth; tests can use `InMemoryMeshtasticAdapter`.
pub trait MeshtasticAdapter {
    /// Start scanning for Meshtastic BLE devices exposing
    /// `MESHTASTIC_BLE_SERVICE_UUID`.
    fn start_scan(&mut self) -> anyhow::Result<Vec<MeshtasticDevice>>;

    /// Connect to a sidecar and subscribe to `FROM_RADIO` notifications.
    fn connect(&mut self, device_id: &MeshtasticDeviceId) -> anyhow::Result<()>;

    /// Tear down GATT/notification state.
    fn disconnect(&mut self) -> anyhow::Result<()>;

    fn is_connected(&self) -> bool;

    /// Send a Meshtastic `ToRadio` protobuf to the sidecar's ToRadio characteristic.
    fn send_to_radio(&mut self, to_radio_protobuf: &[u8]) -> anyhow::Result<()>;

    /// Receive a Meshtastic `FromRadio` protobuf from the sidecar's FromRadio notifications.
    fn try_recv_from_radio(&mut self) -> anyhow::Result<Option<Vec<u8>>>;
}

#[derive(Default)]
pub struct InMemoryMeshtasticAdapter {
    discovered: Vec<MeshtasticDevice>,
    connected: Option<MeshtasticDeviceId>,
    outbound: Vec<Vec<u8>>,
    inbound: VecDeque<Vec<u8>>,
}

impl InMemoryMeshtasticAdapter {
    pub fn with_device(id: impl Into<String>) -> Self {
        let id = MeshtasticDeviceId(id.into());
        Self {
            discovered: vec![MeshtasticDevice {
                id,
                name: Some("mock-meshtastic".to_string()),
                rssi: Some(-42),
            }],
            connected: None,
            outbound: Vec::new(),
            inbound: VecDeque::new(),
        }
    }

    pub fn push_inbound(&mut self, payload: Vec<u8>) {
        self.inbound.push_back(payload);
    }

    pub fn outbound(&self) -> &[Vec<u8>] {
        &self.outbound
    }
}

impl MeshtasticAdapter for InMemoryMeshtasticAdapter {
    fn start_scan(&mut self) -> anyhow::Result<Vec<MeshtasticDevice>> {
        Ok(self.discovered.clone())
    }

    fn connect(&mut self, device_id: &MeshtasticDeviceId) -> anyhow::Result<()> {
        if self.discovered.iter().any(|device| &device.id == device_id) {
            self.connected = Some(device_id.clone());
            Ok(())
        } else {
            anyhow::bail!("Meshtastic sidecar not discovered: {}", device_id.0)
        }
    }

    fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connected = None;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.is_some()
    }

    fn send_to_radio(&mut self, to_radio_protobuf: &[u8]) -> anyhow::Result<()> {
        if !self.is_connected() {
            anyhow::bail!("Meshtastic sidecar is not connected");
        }
        self.outbound.push(to_radio_protobuf.to_vec());
        Ok(())
    }

    fn try_recv_from_radio(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        if !self.is_connected() {
            return Ok(None);
        }
        Ok(self.inbound.pop_front())
    }
}

#[derive(Default)]
pub struct MeshDedupe {
    seen: HashSet<PacketKey>,
}

impl MeshDedupe {
    /// Returns true when this topic/message pair has not been seen before.
    pub fn remember(
        &mut self,
        topic_tag: [u8; TOPIC_TAG_BYTES],
        message_id: [u8; MESSAGE_ID_BYTES],
    ) -> bool {
        self.seen.insert(PacketKey {
            topic_tag,
            message_id,
        })
    }
}

pub struct MeshtasticSidecar<A> {
    adapter: A,
    config: MeshtasticSidecarConfig,
    state: MeshtasticConnectionState,
    tx_queue: VecDeque<OffbeatSyncFrame>,
    reassembly: MeshtasticReassembly,
    dedupe: MeshDedupe,
    tx_packets: u64,
    rx_packets: u64,
    rx_frames: u64,
    dropped_duplicates: u64,
    last_error: Option<String>,
}

impl<A: MeshtasticAdapter> MeshtasticSidecar<A> {
    pub fn new(adapter: A, config: MeshtasticSidecarConfig) -> Self {
        Self {
            adapter,
            config,
            state: MeshtasticConnectionState::Stopped,
            tx_queue: VecDeque::new(),
            reassembly: MeshtasticReassembly::default(),
            dedupe: MeshDedupe::default(),
            tx_packets: 0,
            rx_packets: 0,
            rx_frames: 0,
            dropped_duplicates: 0,
            last_error: None,
        }
    }

    pub fn adapter(&self) -> &A {
        &self.adapter
    }

    pub fn adapter_mut(&mut self) -> &mut A {
        &mut self.adapter
    }

    pub fn status(&self) -> MeshtasticStatus {
        MeshtasticStatus {
            state: self.state.clone(),
            queued_frames: self.tx_queue.len(),
            tx_packets: self.tx_packets,
            rx_packets: self.rx_packets,
            rx_frames: self.rx_frames,
            dropped_duplicates: self.dropped_duplicates,
            last_error: self.last_error.clone(),
        }
    }

    /// Start lifecycle management: scan and connect immediately to the
    /// preferred sidecar or the first discovered Meshtastic BLE device.
    pub fn start(&mut self) -> anyhow::Result<()> {
        self.state = MeshtasticConnectionState::Scanning;
        let devices = self.adapter.start_scan()?;
        let target = match &self.config.preferred_device {
            Some(preferred) => devices
                .iter()
                .find(|device| &device.id == preferred)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("preferred Meshtastic sidecar not found"))?,
            None => devices
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("no Meshtastic sidecar discovered"))?,
        };
        self.connect(target.id)
    }

    pub fn connect(&mut self, device_id: MeshtasticDeviceId) -> anyhow::Result<()> {
        self.state = MeshtasticConnectionState::Connecting {
            device_id: device_id.clone(),
        };
        match self.adapter.connect(&device_id) {
            Ok(()) => {
                self.state = MeshtasticConnectionState::Connected { device_id };
                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                let error = e.to_string();
                self.state = MeshtasticConnectionState::Failed {
                    error: error.clone(),
                };
                self.last_error = Some(error);
                Err(e)
            }
        }
    }

    pub fn stop(&mut self) -> anyhow::Result<()> {
        self.adapter.disconnect()?;
        self.state = MeshtasticConnectionState::Stopped;
        self.tx_queue.clear();
        Ok(())
    }

    /// Queue a logical constrained sync frame. Call `tick` to packetize and send.
    pub fn queue_frame(&mut self, frame: OffbeatSyncFrame) -> anyhow::Result<()> {
        if self.tx_queue.len() >= self.config.max_tx_queue {
            anyhow::bail!("Meshtastic TX queue full");
        }
        self.tx_queue.push_back(frame);
        Ok(())
    }

    /// Drive one lifecycle iteration: reconnect if needed, flush TX, and poll RX.
    pub fn tick(&mut self) -> anyhow::Result<Vec<OffbeatSyncFrame>> {
        if !self.adapter.is_connected() {
            self.state = MeshtasticConnectionState::Backoff { attempt: 1 };
            if self.config.preferred_device.is_some() {
                self.start()?;
            }
        }
        self.flush_tx()?;
        self.poll_rx()
    }

    fn flush_tx(&mut self) -> anyhow::Result<()> {
        if !self.adapter.is_connected() {
            return Ok(());
        }
        while let Some(frame) = self.tx_queue.pop_front() {
            for payload in frame.encode_private_app_payloads()? {
                let to_radio =
                    encode_to_radio_private_app(payload, frame.kind.priority(), frame.ttl, false)?;
                self.adapter.send_to_radio(&to_radio)?;
                self.tx_packets += 1;
            }
        }
        Ok(())
    }

    pub fn poll_rx(&mut self) -> anyhow::Result<Vec<OffbeatSyncFrame>> {
        let mut frames = Vec::new();
        while let Some(from_radio) = self.adapter.try_recv_from_radio()? {
            self.rx_packets += 1;
            let Some(payload) = decode_from_radio_private_app(&from_radio)? else {
                continue;
            };
            let packet = MeshtasticPacket::decode(&payload)?;
            if let Some(frame) = self.reassembly.push(packet)? {
                if self.dedupe.remember(frame.topic_tag, frame.message_id) {
                    self.rx_frames += 1;
                    frames.push(frame);
                } else {
                    self.dropped_duplicates += 1;
                }
            }
        }
        Ok(frames)
    }
}

fn kind_to_u8(kind: SyncPayloadKind) -> u8 {
    match kind {
        SyncPayloadKind::FestivalUpdate => 0,
        SyncPayloadKind::GroupUpdate => 1,
        SyncPayloadKind::GroupChat => 2,
        SyncPayloadKind::FestivalChat => 3,
        SyncPayloadKind::BulkCrdtSync => 4,
        SyncPayloadKind::ChatHistory => 5,
    }
}

fn kind_from_u8(value: u8) -> anyhow::Result<SyncPayloadKind> {
    match value {
        0 => Ok(SyncPayloadKind::FestivalUpdate),
        1 => Ok(SyncPayloadKind::GroupUpdate),
        2 => Ok(SyncPayloadKind::GroupChat),
        3 => Ok(SyncPayloadKind::FestivalChat),
        4 => Ok(SyncPayloadKind::BulkCrdtSync),
        5 => Ok(SyncPayloadKind::ChatHistory),
        _ => anyhow::bail!("unknown sync payload kind {value}"),
    }
}

fn priority_to_u8(priority: SyncPriority) -> u8 {
    match priority {
        SyncPriority::P0Critical => 0,
        SyncPriority::P1High => 1,
        SyncPriority::P2Normal => 2,
        SyncPriority::P3Idle => 3,
    }
}

fn priority_from_u8(value: u8) -> anyhow::Result<SyncPriority> {
    match value {
        0 => Ok(SyncPriority::P0Critical),
        1 => Ok(SyncPriority::P1High),
        2 => Ok(SyncPriority::P2Normal),
        3 => Ok(SyncPriority::P3Idle),
        _ => anyhow::bail!("unknown sync priority {value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPIC: [u8; TOPIC_TAG_BYTES] = *b"topic123";
    const MSG: [u8; MESSAGE_ID_BYTES] = *b"msgid123";

    #[test]
    fn compact_group_chat_round_trips_and_tag_is_keyed() {
        let key_a = [7u8; 32];
        let key_b = [8u8; 32];
        assert_ne!(group_chat_topic_tag(&key_a), group_chat_topic_tag(&key_b));

        let chat = CompactGroupChat {
            message_uuid: [1u8; 16],
            user_id: "user-1".to_string(),
            display_name: "Alice".to_string(),
            text: "meet left of the main stage".to_string(),
            writer_seq: 42,
            logical_time: 99,
            timestamp_secs: 123456,
        };
        let encoded = chat.encode().unwrap();
        assert!(encoded.len() < 160);
        assert_eq!(CompactGroupChat::decode(&encoded).unwrap(), chat);

        let mut legacy = vec![LEGACY_COMPACT_GROUP_CHAT_VERSION];
        legacy.extend_from_slice(&[2u8; 16]);
        legacy.extend_from_slice(&7u64.to_be_bytes());
        legacy.extend_from_slice(&123u64.to_be_bytes());
        legacy.extend_from_slice(&[1, 1, 2]);
        legacy.extend_from_slice(b"uNhi");
        let decoded_legacy = CompactGroupChat::decode(&legacy).unwrap();
        assert_eq!(decoded_legacy.writer_seq, 7);
        assert_eq!(decoded_legacy.logical_time, 7);
    }

    #[test]
    fn private_app_packet_round_trips_as_constrained_sync_payload() {
        let frame = OffbeatSyncFrame::new(
            SyncPayloadKind::GroupUpdate,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            b"compact group op".to_vec(),
        )
        .unwrap();
        let payloads = frame.encode_private_app_payloads().unwrap();
        assert_eq!(payloads.len(), 1);
        assert!(payloads[0].len() <= SAFE_PRIVATE_APP_PAYLOAD_BYTES);

        let packet = MeshtasticPacket::decode(&payloads[0]).unwrap();
        assert_eq!(packet.topic_tag, TOPIC);
        assert_eq!(packet.message_id, MSG);
        assert_eq!(packet.priority, SyncPriority::P1High);

        let mut reassembly = MeshtasticReassembly::default();
        let decoded = reassembly.push(packet).unwrap().unwrap();
        assert_eq!(decoded, frame);
        assert_eq!(decoded.profile, TransportProfile::Constrained);
    }

    #[test]
    fn packet_protocol_fragments_and_reassembles() {
        let frame = OffbeatSyncFrame::new(
            SyncPayloadKind::GroupChat,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            vec![42u8; 120],
        )
        .unwrap();
        let payloads = frame.encode_private_app_payloads_with_mtu(70).unwrap();
        assert!(payloads.len() > 1);
        assert!(payloads.iter().all(|payload| payload.len() <= 70));

        let mut reassembly = MeshtasticReassembly::default();
        let mut decoded = None;
        for payload in payloads {
            decoded = reassembly
                .push(MeshtasticPacket::decode(&payload).unwrap())
                .unwrap();
        }
        assert_eq!(decoded.unwrap(), frame);
    }

    #[test]
    fn meshtastic_rejects_bulk_sync_payloads() {
        let err = OffbeatSyncFrame::new(
            SyncPayloadKind::BulkCrdtSync,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            b"yrs diff".to_vec(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("suppressed"));
    }

    #[test]
    fn meshtastic_rejects_oversize_compact_payloads() {
        let err = OffbeatSyncFrame::new(
            SyncPayloadKind::GroupChat,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            vec![0u8; MESHTASTIC_PROFILE.max_payload_bytes() + 1],
        )
        .unwrap_err();
        assert!(err.to_string().contains("max"));
    }

    #[test]
    fn dedupe_is_topic_scoped() {
        let mut dedupe = MeshDedupe::default();
        assert!(dedupe.remember(TOPIC, MSG));
        assert!(!dedupe.remember(TOPIC, MSG));
        assert!(dedupe.remember(*b"topic124", MSG));
    }

    #[test]
    fn sidecar_lifecycle_connects_flushes_and_receives() {
        let adapter = InMemoryMeshtasticAdapter::with_device("radio-a");
        let mut sidecar = MeshtasticSidecar::new(adapter, MeshtasticSidecarConfig::default());
        sidecar.start().unwrap();
        assert!(matches!(
            sidecar.status().state,
            MeshtasticConnectionState::Connected { .. }
        ));

        let frame = OffbeatSyncFrame::new(
            SyncPayloadKind::FestivalUpdate,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            b"alert".to_vec(),
        )
        .unwrap();
        sidecar.queue_frame(frame.clone()).unwrap();
        let received = sidecar.tick().unwrap();
        assert!(received.is_empty());
        assert_eq!(sidecar.adapter().outbound().len(), 1);
        assert_eq!(sidecar.status().tx_packets, 1);
        assert_eq!(
            decode_to_radio_private_app(&sidecar.adapter().outbound()[0])
                .unwrap()
                .unwrap(),
            frame.encode_private_app_payloads().unwrap().remove(0)
        );

        let payload = frame.encode_private_app_payloads().unwrap().remove(0);
        sidecar
            .adapter_mut()
            .push_inbound(encode_from_radio_private_app(payload));
        let received = sidecar.tick().unwrap();
        assert_eq!(received, vec![frame]);
        assert_eq!(sidecar.status().rx_packets, 1);
        assert_eq!(sidecar.status().rx_frames, 1);
    }

    #[test]
    fn sidecar_dedupes_duplicate_inbound_frames() {
        let adapter = InMemoryMeshtasticAdapter::with_device("radio-a");
        let mut sidecar = MeshtasticSidecar::new(adapter, MeshtasticSidecarConfig::default());
        sidecar.start().unwrap();
        let payload = OffbeatSyncFrame::new(
            SyncPayloadKind::GroupUpdate,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            b"presence".to_vec(),
        )
        .unwrap()
        .encode_private_app_payloads()
        .unwrap()
        .remove(0);
        sidecar
            .adapter_mut()
            .push_inbound(encode_from_radio_private_app(payload.clone()));
        sidecar
            .adapter_mut()
            .push_inbound(encode_from_radio_private_app(payload));
        let received = sidecar.tick().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(sidecar.status().dropped_duplicates, 1);
    }

    #[test]
    fn from_radio_ignores_non_private_app_packets() {
        let data = meshtastic::Data {
            portnum: meshtastic::PortNum::TextMessageApp as i32,
            payload: b"not offbeat".to_vec(),
            ..Default::default()
        };
        let packet = meshtastic::MeshPacket {
            payload_variant: Some(mesh_packet::PayloadVariant::Decoded(data)),
            ..Default::default()
        };
        let from_radio = meshtastic::FromRadio {
            id: 1,
            payload_variant: Some(from_radio::PayloadVariant::Packet(packet)),
        }
        .encode_to_vec();
        assert!(decode_from_radio_private_app(&from_radio)
            .unwrap()
            .is_none());
    }

    #[test]
    fn malformed_packets_are_rejected_before_reassembly() {
        let mut raw = OffbeatSyncFrame::new(
            SyncPayloadKind::FestivalUpdate,
            TOPIC,
            MSG,
            DEFAULT_TTL,
            b"alert".to_vec(),
        )
        .unwrap()
        .encode_private_app_payloads()
        .unwrap()
        .remove(0);
        raw[0] = b'X';
        assert!(MeshtasticPacket::decode(&raw).is_err());
    }
}
