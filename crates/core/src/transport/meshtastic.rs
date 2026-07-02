//! Meshtastic route adapter.
//!
//! Meshtastic is modeled as a constrained physical path for the shared Offbeat
//! sync protocol, not as a separate semantic event lane. The bytes carried over
//! `PRIVATE_APP` are compact encodings of normal Offbeat resource payloads
//! selected by `TransportProfile::Constrained`.
//!
//! Hardware integration is expected to sit behind `MeshtasticAdapter`:
//! phone ⇄ Bluetooth ⇄ Meshtastic card ⇄ LoRa mesh ⇄ Meshtastic card ⇄ Bluetooth ⇄ phone.

use std::collections::{HashSet, VecDeque};

use super::profile::{SyncEncoding, SyncPayloadKind, TransportProfile};

pub const MESHTASTIC_PROFILE: TransportProfile = TransportProfile::Constrained;
pub const FRAME_VERSION: u8 = 1;
pub const DEFAULT_TTL: u8 = 3;
pub const MESSAGE_ID_BYTES: usize = 8;
pub const HEADER_BYTES: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffbeatSyncFrame {
    pub version: u8,
    pub profile: TransportProfile,
    pub kind: SyncPayloadKind,
    pub message_id: [u8; MESSAGE_ID_BYTES],
    pub ttl: u8,
    /// Compact body for `kind`; already signed/encrypted by higher layers where required.
    pub body: Vec<u8>,
}

impl OffbeatSyncFrame {
    pub fn new(
        kind: SyncPayloadKind,
        message_id: [u8; MESSAGE_ID_BYTES],
        ttl: u8,
        body: Vec<u8>,
    ) -> anyhow::Result<Self> {
        match MESHTASTIC_PROFILE.decide(kind).encoding {
            SyncEncoding::CompactFrame { max_bytes } if body.len() <= max_bytes => Ok(Self {
                version: FRAME_VERSION,
                profile: MESHTASTIC_PROFILE,
                kind,
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

    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        if self.version != FRAME_VERSION {
            anyhow::bail!("unsupported frame version {}", self.version);
        }
        let max_bytes = match self.profile.decide(self.kind).encoding {
            SyncEncoding::CompactFrame { max_bytes } => max_bytes,
            _ => anyhow::bail!(
                "frame kind {:?} is not compact for {:?}",
                self.kind,
                self.profile
            ),
        };
        if self.body.len() > max_bytes {
            anyhow::bail!(
                "compact Meshtastic body is {} bytes; max is {max_bytes}",
                self.body.len()
            );
        }
        if self.body.len() > u16::MAX as usize {
            anyhow::bail!("body too large for frame length");
        }

        let mut out = Vec::with_capacity(HEADER_BYTES + self.body.len());
        out.push(self.version);
        out.push(profile_to_u8(self.profile));
        out.push(kind_to_u8(self.kind));
        out.push(self.ttl);
        out.extend_from_slice(&self.message_id);
        out.extend_from_slice(&(self.body.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.body);
        Ok(out)
    }

    pub fn decode(raw: &[u8]) -> anyhow::Result<Self> {
        if raw.len() < HEADER_BYTES {
            anyhow::bail!("frame too short");
        }
        let version = raw[0];
        if version != FRAME_VERSION {
            anyhow::bail!("unsupported frame version {version}");
        }
        let profile = profile_from_u8(raw[1])?;
        if profile != MESHTASTIC_PROFILE {
            anyhow::bail!("Meshtastic adapter received {profile:?} frame");
        }
        let kind = kind_from_u8(raw[2])?;
        let ttl = raw[3];
        let mut message_id = [0u8; MESSAGE_ID_BYTES];
        message_id.copy_from_slice(&raw[4..12]);
        let body_len = u16::from_be_bytes([raw[12], raw[13]]) as usize;
        if raw.len() != HEADER_BYTES + body_len {
            anyhow::bail!("frame body length mismatch");
        }
        Self::new(kind, message_id, ttl, raw[HEADER_BYTES..].to_vec())
    }
}

/// Hardware-facing adapter. Production implementations talk to a paired
/// Meshtastic device over Bluetooth; tests can use `InMemoryMeshtasticAdapter`.
pub trait MeshtasticAdapter {
    fn send_private_app(&mut self, frame: &[u8]) -> anyhow::Result<()>;
    fn try_recv_private_app(&mut self) -> anyhow::Result<Option<Vec<u8>>>;
}

#[derive(Default)]
pub struct InMemoryMeshtasticAdapter {
    outbound: Vec<Vec<u8>>,
    inbound: VecDeque<Vec<u8>>,
}

impl InMemoryMeshtasticAdapter {
    pub fn push_inbound(&mut self, frame: Vec<u8>) {
        self.inbound.push_back(frame);
    }

    pub fn outbound(&self) -> &[Vec<u8>] {
        &self.outbound
    }
}

impl MeshtasticAdapter for InMemoryMeshtasticAdapter {
    fn send_private_app(&mut self, frame: &[u8]) -> anyhow::Result<()> {
        self.outbound.push(frame.to_vec());
        Ok(())
    }

    fn try_recv_private_app(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.inbound.pop_front())
    }
}

#[derive(Default)]
pub struct MeshDedupe {
    seen: HashSet<[u8; MESSAGE_ID_BYTES]>,
}

impl MeshDedupe {
    /// Returns true when this message id has not been seen before.
    pub fn remember(&mut self, message_id: [u8; MESSAGE_ID_BYTES]) -> bool {
        self.seen.insert(message_id)
    }
}

fn profile_to_u8(profile: TransportProfile) -> u8 {
    match profile {
        TransportProfile::Constrained => 0,
        TransportProfile::LowBandwidth => 1,
        TransportProfile::Full => 2,
    }
}

fn profile_from_u8(value: u8) -> anyhow::Result<TransportProfile> {
    match value {
        0 => Ok(TransportProfile::Constrained),
        1 => Ok(TransportProfile::LowBandwidth),
        2 => Ok(TransportProfile::Full),
        _ => anyhow::bail!("unknown transport profile {value}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trips_as_constrained_sync_payload() {
        let frame = OffbeatSyncFrame::new(
            SyncPayloadKind::GroupUpdate,
            *b"12345678",
            DEFAULT_TTL,
            b"compact group op".to_vec(),
        )
        .unwrap();
        let decoded = OffbeatSyncFrame::decode(&frame.encode().unwrap()).unwrap();
        assert_eq!(decoded, frame);
        assert_eq!(decoded.profile, TransportProfile::Constrained);
    }

    #[test]
    fn meshtastic_rejects_bulk_sync_payloads() {
        let err = OffbeatSyncFrame::new(
            SyncPayloadKind::BulkCrdtSync,
            *b"12345678",
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
            *b"12345678",
            DEFAULT_TTL,
            vec![0u8; MESHTASTIC_PROFILE.max_payload_bytes() + 1],
        )
        .unwrap_err();
        assert!(err.to_string().contains("max"));
    }

    #[test]
    fn dedupe_accepts_first_delivery_only() {
        let mut dedupe = MeshDedupe::default();
        assert!(dedupe.remember(*b"12345678"));
        assert!(!dedupe.remember(*b"12345678"));
    }
}
