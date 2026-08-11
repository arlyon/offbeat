---
{
  "schemaVersion": 1,
  "id": "offbeat.wifi-aware",
  "locale": "en",
  "title": "OFFBEAT Wi-Fi Aware",
  "summary": "Understand the planned nearby high-bandwidth route and why it is not currently available for setup or sync.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["WiFi Aware", "Wi-Fi Direct", "NAN", "nearby Wi-Fi sync"],
  "tags": ["Wi-Fi Aware", "planned", "Android", "P2P", "offline"],
  "generatedRefs": [],
  "priority": "normal",
  "order": 860,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Wi-Fi Aware Rust scaffold",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/transport/wifi_aware.rs"
    },
    {
      "title": "Android Wi-Fi Aware capability probe",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/android/app/src/main/kotlin/com/offbeat/offbeat_mobile/WifiAwareBridge.kt"
    },
    {
      "title": "Android manifest",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/android/app/src/main/AndroidManifest.xml"
    },
    {
      "title": "iOS app configuration",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/ios/Runner/Info.plist"
    },
    {
      "title": "Direct connectivity requirements",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/docs/prd-p2p-direct-connectivity.md"
    }
  ]
}
---
# OFFBEAT Wi-Fi Aware

> **Unavailable:** OFFBEAT does not currently provide a working Wi-Fi Aware or Wi-Fi Direct sync route. There is no user setup flow to follow.

Wi-Fi Aware, also called Neighbor Awareness Networking on Android, could let nearby compatible devices discover each other and establish a high-bandwidth local data path without joining the same access point. OFFBEAT models such a future path as a full sync route for CRDT catch-up and bounded chat history.

## Current implementation status

The repository contains:

- A Rust transport/status scaffold and full-route policy model.
- An Android capability probe for operating-system version, hardware feature and permissions.
- Android manifest declarations for nearby Wi-Fi and Wi-Fi Aware hardware.
- An iOS service declaration placeholder.
- Unit tests for the abstract route model.

It does not contain the required publish/subscribe discovery, Android data-path establishment, iOS platform bridge, iroh route handoff, normal Flutter status, or multi-device field validation. A capability probe is not a connection.

Wi-Fi Direct is also not implemented as a fallback.

## What it may provide later

If completed and validated, a full local Wi-Fi route could carry the same OFFBEAT resources as other capable transports:

- Signed festival checkpoints and updates.
- Encrypted group CRDT state.
- Bounded public and group chat catch-up.
- Live gossip after catch-up.

It would not create a second lineup or group model. The same festival signatures, group keys and endpoint identity checks would still apply.

## Prerequisites for a future route

These are design constraints, not current setup steps:

- Both devices would need platform and hardware support.
- Android support would require a Wi-Fi Aware capable device and the relevant nearby Wi-Fi and location permissions expected by the current probe.
- The app would need to be allowed to advertise, discover and establish a peer data path.
- Both devices would need compatible OFFBEAT protocol versions and a shared known festival.
- Private resources would still require the same group key.

Because none of this is wired into the current product, changing Wi-Fi or permission settings cannot enable OFFBEAT Wi-Fi Aware today.

## Offline behavior

There is no current Wi-Fi Aware offline behavior. OFFBEAT continues to use local storage and any other available routes, such as the internet relay or integrated Bluetooth path. If those routes are absent, state remains local.

The planned route would not provide internet and would not discover a festival that the device had never seen.

## Privacy and trust

A future nearby service advertisement could reveal that an OFFBEAT device is present. The scaffold uses app-scoped peer hints rather than promising access to stable hardware identifiers, but actual platform privacy behavior still needs implementation and testing.

Even on a local Wi-Fi path:

- Endpoint identity would authenticate the transport peer only.
- Festival state would still require the festival authority signature.
- Group content would remain encrypted with the group key.
- Traffic timing, device proximity and service discovery could reveal metadata.

## Battery and platform constraints

Discovery and peer data paths can consume power. Operating systems may limit background discovery, prompt for consent or stop a path when the app is suspended. Device support is optional even on an otherwise compatible Android version.

OFFBEAT has no measured battery, latency, compatibility or fallback guarantees for this route.

## Constraints

- No current OFFBEAT screen, bridge or sync task establishes a Wi-Fi Aware route.
- Hardware declarations and capability checks do not prove peer discovery or data transfer.
- Wi-Fi Direct is not implemented as a substitute.
- Platform support, user consent, background behavior and battery cost remain unvalidated.
- The route cannot be enabled by changing settings in the current app.

## Troubleshooting

There is no Wi-Fi Aware control in the current OFFBEAT UI. If you expected one:

1. Do not repeatedly toggle Wi-Fi, location or nearby-device permissions in an attempt to reveal it.
2. Use the connection drawer to inspect the routes that actually exist: WebSocket relay and Bluetooth LE.
3. Keep using cached local data if no route is available.
4. Do not install third-party Wi-Fi Aware tools or create an ad hoc hotspot as an OFFBEAT workaround. Those do not wire the missing application protocol.
5. Recheck this page after an OFFBEAT update that explicitly announces production Wi-Fi Aware support.

For today's local route, see [Bluetooth sync](wiki:offbeat.bluetooth-sync). For the wider model, see [P2P syncing](wiki:offbeat.p2p-syncing).
