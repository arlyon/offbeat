//! Wi-Fi Aware / NAN route scaffold.
//!
//! Wi-Fi Aware is Offbeat's primary no-access-point high-bandwidth route. It is
//! modeled as a `TransportProfile::Full` path: once a platform data path exists,
//! normal Offbeat gossip, Yrs state-vector exchange, chat catch-up, and future
//! local calls can run over it.
//!
//! Platform expectations:
//! - Android: `WifiAwareManager` publish/subscribe plus NDP (API 26+, hardware gated).
//! - iOS: WiFiAware + Network.framework (iOS 26+, supported hardware, entitlement,
//!   Info.plist service declaration, DeviceDiscoveryUI pairing).
//!
//! The implementation boundary is deliberately small: platform code establishes
//! discovery/pairing and either yields native IP/UDP route hints for iroh or, if
//! the OS only exposes stream/message APIs, backs a future iroh custom transport.

use std::net::SocketAddr;

use super::profile::{SyncEncoding, SyncPayloadKind, TransportProfile};

/// RFC 6763 service name component must be <= 15 chars. `offbeat-sync` is 12.
pub const WIFI_AWARE_SERVICE_NAME: &str = "_offbeat-sync._udp";

pub const WIFI_AWARE_PROFILE: TransportProfile = TransportProfile::Full;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WifiAwarePlatform {
    Android,
    Ios,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WifiAwareRouteMode {
    /// Platform data path exposes socket addresses usable by the normal iroh endpoint.
    NativeIp,
    /// Platform only exposes a connection object; adapt it behind iroh custom transport.
    CustomTransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiAwareCapability {
    pub platform: WifiAwarePlatform,
    pub os_supported: bool,
    /// None means Rust cannot know yet; ask the platform bridge at runtime.
    pub hardware_supported: Option<bool>,
    pub service_declared: bool,
    pub entitlement_declared: bool,
    pub reason: Option<String>,
}

impl WifiAwareCapability {
    pub fn available(platform: WifiAwarePlatform) -> Self {
        Self {
            platform,
            os_supported: true,
            hardware_supported: None,
            service_declared: true,
            entitlement_declared: true,
            reason: None,
        }
    }

    pub fn unavailable(platform: WifiAwarePlatform, reason: impl Into<String>) -> Self {
        Self {
            platform,
            os_supported: false,
            hardware_supported: Some(false),
            service_declared: false,
            entitlement_declared: false,
            reason: Some(reason.into()),
        }
    }

    pub fn is_potentially_available(&self) -> bool {
        self.os_supported
            && self.service_declared
            && self.entitlement_declared
            && self.hardware_supported != Some(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiAwarePeerHint {
    /// App-scoped platform peer id. Do not expose stable hardware ids.
    pub peer_id: String,
    /// Optional iroh EndpointId if exchanged during discovery/pairing.
    pub endpoint_id: Option<String>,
    /// Native route hint if the platform data path yields an IP socket address.
    pub socket_addr: Option<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiAwareStatus {
    pub capability: WifiAwareCapability,
    pub active: bool,
    pub route_mode: Option<WifiAwareRouteMode>,
    pub peers: Vec<WifiAwarePeerHint>,
}

impl WifiAwareStatus {
    pub fn inactive(capability: WifiAwareCapability) -> Self {
        Self {
            capability,
            active: false,
            route_mode: None,
            peers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WifiAwareTransport {
    status: WifiAwareStatus,
}

impl WifiAwareTransport {
    pub fn new(capability: WifiAwareCapability) -> Self {
        Self {
            status: WifiAwareStatus::inactive(capability),
        }
    }

    pub fn profile(&self) -> TransportProfile {
        WIFI_AWARE_PROFILE
    }

    pub fn status(&self) -> &WifiAwareStatus {
        &self.status
    }

    pub fn set_route_mode(&mut self, route_mode: WifiAwareRouteMode) {
        self.status.route_mode = Some(route_mode);
        self.status.active = true;
    }

    pub fn replace_peers(&mut self, peers: Vec<WifiAwarePeerHint>) {
        self.status.peers = peers;
        self.status.active = !self.status.peers.is_empty();
    }

    pub fn discovered_socket_addrs(&self) -> impl Iterator<Item = SocketAddr> + '_ {
        self.status.peers.iter().filter_map(|peer| peer.socket_addr)
    }

    pub fn encoding_for(&self, kind: SyncPayloadKind) -> SyncEncoding {
        self.profile().decide(kind).encoding
    }
}

/// Static platform capability. Real hardware/permission availability comes from
/// Android/iOS bridge code and should update `WifiAwareStatus` at runtime.
pub fn platform_capability() -> WifiAwareCapability {
    #[cfg(target_os = "android")]
    {
        return WifiAwareCapability::available(WifiAwarePlatform::Android);
    }

    #[cfg(target_os = "ios")]
    {
        return WifiAwareCapability::available(WifiAwarePlatform::Ios);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        WifiAwareCapability::unavailable(
            WifiAwarePlatform::Unsupported,
            "Wi-Fi Aware is only supported on Android/iOS mobile targets",
        )
    }
}

pub async fn try_build_wifi_aware() -> Option<WifiAwareTransport> {
    let capability = platform_capability();
    capability
        .is_potentially_available()
        .then(|| WifiAwareTransport::new(capability))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_aware_is_full_sync_profile() {
        let transport =
            WifiAwareTransport::new(WifiAwareCapability::available(WifiAwarePlatform::Android));
        assert_eq!(transport.profile(), TransportProfile::Full);
        assert_eq!(
            transport.encoding_for(SyncPayloadKind::FestivalUpdate),
            SyncEncoding::FullEnvelope
        );
        assert_eq!(
            transport.encoding_for(SyncPayloadKind::ChatHistory),
            SyncEncoding::FullEnvelope
        );
        assert!(transport.profile().allows_chat_catchup());
    }

    #[test]
    fn capability_requires_runtime_hardware_not_false() {
        let mut capability = WifiAwareCapability::available(WifiAwarePlatform::Ios);
        assert!(capability.is_potentially_available());
        capability.hardware_supported = Some(false);
        assert!(!capability.is_potentially_available());
    }

    #[test]
    fn native_ip_peers_yield_socket_hints_for_iroh() {
        let mut transport =
            WifiAwareTransport::new(WifiAwareCapability::available(WifiAwarePlatform::Android));
        let addr: SocketAddr = "192.0.2.10:7777".parse().unwrap();
        transport.set_route_mode(WifiAwareRouteMode::NativeIp);
        transport.replace_peers(vec![WifiAwarePeerHint {
            peer_id: "peer-a".to_string(),
            endpoint_id: None,
            socket_addr: Some(addr),
        }]);
        assert_eq!(
            transport.discovered_socket_addrs().collect::<Vec<_>>(),
            vec![addr]
        );
    }
}
