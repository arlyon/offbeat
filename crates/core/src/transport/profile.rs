//! Transport-aware sync policy.
//!
//! All physical routes participate in the same Offbeat resource sync model, but
//! they do not all carry the same wire encoding. High-bandwidth routes can send
//! full gossip envelopes, state-vector exchanges, and append-log catch-up. BLE
//! uses bounded forms. Meshtastic/LoRa is a constrained profile that carries
//! compact encodings of the same logical resources instead of a separate
//! application-level event lane.

/// Bandwidth/cost class for a peer path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransportProfile {
    /// Very small, high-latency, shared-airtime links such as Meshtastic/LoRa.
    Constrained,
    /// Low bandwidth but interactive links such as BLE.
    LowBandwidth,
    /// Full sync routes such as WebSocket, LAN, Wi-Fi Aware, Wi-Fi Direct, or AWDL.
    Full,
}

/// Logical Offbeat resource payloads the sync layer schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncPayloadKind {
    FestivalUpdate,
    GroupUpdate,
    GroupChat,
    FestivalChat,
    BulkCrdtSync,
    ChatHistory,
}

/// Scheduler priority. Lower variants are sent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncPriority {
    P0Critical,
    P1High,
    P2Normal,
    P3Idle,
}

/// Wire encoding selected for a logical payload on a particular path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncEncoding {
    /// Send the existing full Offbeat gossip/sync envelope.
    FullEnvelope,
    /// Send an existing envelope only when it is small enough for this path.
    BoundedEnvelope { max_bytes: usize },
    /// Send a compact profile-specific body for the same logical resource.
    CompactFrame { max_bytes: usize },
    /// Do not send this payload on the selected path.
    Suppressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyncPolicyDecision {
    pub priority: SyncPriority,
    pub encoding: SyncEncoding,
}

impl SyncPayloadKind {
    pub fn priority(self) -> SyncPriority {
        match self {
            Self::FestivalUpdate => SyncPriority::P0Critical,
            Self::GroupUpdate => SyncPriority::P1High,
            Self::GroupChat => SyncPriority::P2Normal,
            Self::FestivalChat | Self::BulkCrdtSync | Self::ChatHistory => SyncPriority::P3Idle,
        }
    }
}

impl TransportProfile {
    /// Conservative app payload budget for this profile before lower-layer framing.
    pub fn max_payload_bytes(self) -> usize {
        match self {
            Self::Full => 8 * 1024 * 1024,
            Self::LowBandwidth => 1200,
            Self::Constrained => 200,
        }
    }

    /// Whether append-log catch-up is appropriate for this profile.
    pub fn allows_chat_catchup(self) -> bool {
        !matches!(self, Self::Constrained)
    }

    /// Maximum catch-up batch size for append logs on this profile.
    pub fn chat_catchup_limit(self) -> u32 {
        match self {
            Self::Full => 200,
            Self::LowBandwidth => 50,
            Self::Constrained => 0,
        }
    }

    /// Select the wire encoding for a logical resource on this path.
    pub fn decide(self, kind: SyncPayloadKind) -> SyncPolicyDecision {
        let encoding = match (self, kind) {
            (Self::Full, _) => SyncEncoding::FullEnvelope,

            (Self::LowBandwidth, SyncPayloadKind::FestivalUpdate)
            | (Self::LowBandwidth, SyncPayloadKind::GroupUpdate)
            | (Self::LowBandwidth, SyncPayloadKind::GroupChat) => SyncEncoding::BoundedEnvelope {
                max_bytes: self.max_payload_bytes(),
            },
            (Self::LowBandwidth, SyncPayloadKind::FestivalChat) => SyncEncoding::Suppressed,
            (Self::LowBandwidth, SyncPayloadKind::BulkCrdtSync)
            | (Self::LowBandwidth, SyncPayloadKind::ChatHistory) => SyncEncoding::BoundedEnvelope {
                max_bytes: self.max_payload_bytes(),
            },

            (Self::Constrained, SyncPayloadKind::FestivalUpdate)
            | (Self::Constrained, SyncPayloadKind::GroupUpdate)
            | (Self::Constrained, SyncPayloadKind::GroupChat)
            | (Self::Constrained, SyncPayloadKind::FestivalChat) => SyncEncoding::CompactFrame {
                max_bytes: self.max_payload_bytes(),
            },
            (Self::Constrained, SyncPayloadKind::BulkCrdtSync)
            | (Self::Constrained, SyncPayloadKind::ChatHistory) => SyncEncoding::Suppressed,
        };

        SyncPolicyDecision {
            priority: kind.priority(),
            encoding,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constrained_profile_uses_compact_logical_payloads() {
        let profile = TransportProfile::Constrained;
        assert_eq!(
            profile.decide(SyncPayloadKind::FestivalUpdate).encoding,
            SyncEncoding::CompactFrame { max_bytes: 200 }
        );
        assert_eq!(
            profile.decide(SyncPayloadKind::GroupUpdate).encoding,
            SyncEncoding::CompactFrame { max_bytes: 200 }
        );
        assert_eq!(
            profile.decide(SyncPayloadKind::GroupChat).encoding,
            SyncEncoding::CompactFrame { max_bytes: 200 }
        );
        assert_eq!(
            profile.decide(SyncPayloadKind::FestivalChat).encoding,
            SyncEncoding::CompactFrame { max_bytes: 200 }
        );
    }

    #[test]
    fn constrained_profile_suppresses_bulk_catchup() {
        let profile = TransportProfile::Constrained;
        assert!(!profile.allows_chat_catchup());
        assert_eq!(profile.chat_catchup_limit(), 0);
        assert_eq!(
            profile.decide(SyncPayloadKind::BulkCrdtSync).encoding,
            SyncEncoding::Suppressed
        );
        assert_eq!(
            profile.decide(SyncPayloadKind::ChatHistory).encoding,
            SyncEncoding::Suppressed
        );
    }

    #[test]
    fn priorities_match_offbeat_route_policy() {
        assert!(
            SyncPayloadKind::FestivalUpdate.priority() < SyncPayloadKind::GroupUpdate.priority()
        );
        assert!(SyncPayloadKind::GroupUpdate.priority() < SyncPayloadKind::GroupChat.priority());
        assert!(SyncPayloadKind::GroupChat.priority() < SyncPayloadKind::FestivalChat.priority());
    }
}
