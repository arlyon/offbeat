---
{
  "schemaVersion": 1,
  "id": "offbeat.bluetooth-sync",
  "locale": "en",
  "title": "OFFBEAT Bluetooth sync",
  "summary": "Use OFFBEAT's nearby Bluetooth route, interpret its status, and understand permission, identity, bandwidth, and background limits.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["BLE sync", "Bluetooth LE", "nearby sync", "phone to phone"],
  "tags": ["Bluetooth", "BLE", "P2P", "permissions", "offline"],
  "generatedRefs": [],
  "priority": "high",
  "order": 850,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "BLE transport construction",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/transport/ble.rs"
    },
    {
      "title": "BLE discovery and sync tasks",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/ble_sync.rs"
    },
    {
      "title": "Mobile BLE lifecycle and status",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/mod.rs"
    },
    {
      "title": "Bluetooth permission service",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/services/bluetooth_service.dart"
    },
    {
      "title": "Android mobile permissions",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/android/app/src/main/AndroidManifest.xml"
    },
    {
      "title": "Connection diagnostics UI",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/widgets/connection_drawer.dart"
    }
  ]
}
---
# OFFBEAT Bluetooth sync

OFFBEAT includes an iroh Bluetooth Low Energy route for nearby OFFBEAT devices. It advertises and scans for compatible peers, verifies the complete iroh endpoint identity, and then lets registered gossip resources use that local path.

This is different from pairing a Meshtastic radio. Normal OFFBEAT Bluetooth sync is a phone-to-phone OFFBEAT route and does not require a Meshtastic node.

## Current support

**Integrated into the normal mobile app:** OFFBEAT attempts to build the BLE transport when the networked Rust node starts, then starts discovery, reconnect and gossip-event tasks. The connection drawer reports transport and peer details.

**Field-validation limited:** the code and mock tests do not guarantee that every Android or iOS device, firmware version or background state works end to end. Treat BLE as an available route when the app reports a verified connected peer, not as a universal promise.

**Separate debug feature:** the `MESHTASTIC TEST RIG` scans a Meshtastic service and requires a radio. It is not the normal OFFBEAT peer transport.

## Prerequisites

- Two nearby phones with compatible OFFBEAT builds.
- Bluetooth powered on for both phones.
- OFFBEAT open and its Rust node running on both devices while diagnosing.
- Both devices already know the same festival.
- For private data, both devices must possess the same group key.
- Runtime permissions granted.

On current Android builds, OFFBEAT requests Bluetooth scan, connect and advertise permissions. The manifest also includes fine location for older platform behavior. On iOS, Bluetooth usage descriptions and central/peripheral background modes are declared, but the operating system still controls scheduling.

## Discovery, identity and connection

A nearby advertisement is only a hint. OFFBEAT can observe a bounded endpoint prefix, connect to the BLE service, read the complete endpoint ID, and ask iroh to connect. It deduplicates candidates and avoids connecting to itself.

Status phases matter:

- Transport `ONLINE` means the BLE subsystem is active.
- An advertising beacon or discovered device is not yet a verified peer.
- `VERIFIED` means the complete endpoint identity was obtained.
- `CONNECTED` means the gossip path is active for that peer.
- A resource still needs subscription and catch-up after the transport connection.

Endpoint identity proves continuity of the iroh peer. It does not make that peer a festival authority or give it a private group key.

## What can sync

BLE is classified as a low-bandwidth route. The transport policy bounds state and group traffic instead of treating BLE like Wi-Fi. Public festival chat is suppressed on this profile, while some state, group and bounded catch-up operations can be attempted.

Do not expect a new installation to discover festivals, complete bulk history, or transfer unlimited data over BLE. OFFBEAT still applies the normal signature, encryption and resource-access rules.

## Offline behavior

Bluetooth can provide a local route when the internet relay is unavailable. Both devices still work from their own local stores. If the local BLE path cannot form, accepted local state remains local until another compatible route appears.

A Bluetooth encounter does not guarantee convergence before the phones separate. Bandwidth, payload caps, app lifetime and peer readiness all matter.

## Privacy and trust

Nearby observers can detect Bluetooth activity. Discovery exposes an app service and bounded identity hints; a verified connection uses the complete public endpoint identity. Public endpoint IDs are not secret, but stable identifiers can have privacy implications.

Private group content remains encrypted with the group key. Festival state remains authority-signed. Discovery by an unrelated OFFBEAT device does not grant access to either.

## Battery and background constraints

Scanning, advertising, connections and retries use phone battery. The app uses low-level background support where declared, but Android and iOS can throttle or suspend radio work based on power policy, app state and device vendor behavior.

For a time-sensitive field test, keep both apps in the foreground and screens awake. Do not assume background sync will continue for a specific duration.

## Constraints

- BLE availability, range, throughput and background duration are device dependent.
- A running transport, advertisement or discovered peer does not prove resource convergence.
- The low-bandwidth profile caps or suppresses some traffic and is not a bulk-history route.
- Bluetooth discovery cannot introduce an unseen festival or bypass group keys.
- End-to-end hardware behavior is not yet validated across a representative device matrix.

## Troubleshooting

### `BLUETOOTH OFF`

Use `OPEN SETTINGS`, turn Bluetooth on, return to OFFBEAT and reopen the connection drawer.

### `PERMISSION REQUIRED`

Grant the requested nearby/Bluetooth permissions. If they were permanently denied, use the operating-system app settings. OFFBEAT cannot scan and advertise without them.

### `NOT STARTED`

Use `START` or restart the OFFBEAT node from the drawer. If the device has no usable BLE hardware or initialization fails, the app continues without this route.

### A device is listed but does not connect

Keep both apps open, select the same festival, move the phones closer and wait through reconnect backoff. A beacon match is not proof of a verified endpoint. Use `NUDGE` once, then `RESTART` if the state is stuck.

### A peer connects but data differs

Check the resource rows and group membership. Connection, topic subscription and catch-up are separate phases. Allow time for catch-up and do not clear local storage.

### Battery drain is high

Close repeated diagnostic sessions, avoid restarting in a loop and use another active route when available. Device-specific battery behavior still requires field measurement.

See [P2P syncing](wiki:offbeat.p2p-syncing) and [OFFBEAT over Meshtastic](wiki:meshtastic.offbeat-over-meshtastic).
