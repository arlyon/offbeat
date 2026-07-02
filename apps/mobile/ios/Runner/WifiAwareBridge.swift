import Foundation
import Network

#if canImport(WiFiAware)
import WiFiAware
#endif

/// iOS Wi-Fi Aware capability notes.
///
/// Production wiring requires iOS 26+, supported hardware (iPhone 12+ class),
/// `com.apple.developer.wifi-aware` entitlement with Publish/Subscribe,
/// `WiFiAwareServices` Info.plist entries, and DeviceDiscoveryUI pairing.
///
/// The bridge should publish/subscribe `_offbeat-sync._udp` via Network.framework
/// and return either native endpoint/socket hints for iroh or a connection handle
/// for an iroh custom transport adapter.
enum WifiAwareBridge {
  static let serviceName = "_offbeat-sync._udp"

  static var frameworkAvailable: Bool {
    #if canImport(WiFiAware)
    if #available(iOS 26.0, *) {
      return true
    }
    return false
    #else
    return false
    #endif
  }
}
