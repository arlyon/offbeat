---
{
  "schemaVersion": 1,
  "id": "meshtastic.offbeat-over-meshtastic",
  "locale": "en",
  "title": "OFFBEAT over Meshtastic",
  "summary": "Understand OFFBEAT's Android Meshtastic test rig, its compact group-chat proof, and the features that remain unavailable.",
  "category": "meshtastic",
  "countryCodes": [],
  "aliases": ["OFFBEAT LoRa", "Meshtastic test rig", "PRIVATE_APP", "mesh group chat"],
  "tags": ["Meshtastic", "experimental", "Android", "group chat", "LoRa"],
  "generatedRefs": [],
  "priority": "high",
  "order": 870,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "OFFBEAT Meshtastic transport",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/transport/meshtastic.rs"
    },
    {
      "title": "Transport profile policy",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/transport/profile.rs"
    },
    {
      "title": "Android Meshtastic debug API",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/mod.rs"
    },
    {
      "title": "Meshtastic test rig UI",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/screens/you/meshtastic_debug_sheet.dart"
    },
    {
      "title": "Meshtastic implementation plan",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/docs/meshtastic-implementation-plan.md"
    },
    {
      "title": "Overview",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/index.mdx"
    },
    {
      "title": "Messages and channels",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/android/user/messages-and-channels.md"
    }
  ]
}
---
# OFFBEAT over Meshtastic

> **Experimental debug feature:** OFFBEAT over Meshtastic is currently an Android test rig. It is not an automatic production sync route.

OFFBEAT can encode compact application frames inside Meshtastic's official `PRIVATE_APP` protobuf payload. An Android phone connects by Bluetooth to a selected Meshtastic radio, and the radio broadcasts the payload over LoRa.

## What works now

The `YOU` screen exposes `MESHTASTIC TEST RIG`. The implemented harness can:

- Scan on Android for radios exposing the official Meshtastic BLE service.
- Connect to a selected radio and inspect its GATT services.
- Send and receive synthetic OFFBEAT debug frames.
- Send one compact encrypted group-chat message for a local group.
- Listen for matching compact group-chat frames and write them into the normal local group chat database.
- Reassemble up to four fragments in the compact protocol foundation.

The compact chat body limits message text to 96 UTF-8 bytes. The UI's character counter is not a guarantee for multi-byte characters, so a message near 96 characters may still be rejected.

## What is unavailable

OFFBEAT does not currently provide:

- A persistent background connection to a selected Meshtastic radio.
- An iOS Meshtastic bridge.
- Automatic forwarding of normal lineup, weather, check-in, group-state or chat changes.
- Inbound application of festival-state or group-state frames.
- Full iroh, QUIC or Yrs snapshot transfer over LoRa.
- Bulk chat history or lineup history over LoRa.
- A restart-safe Meshtastic outbound queue.
- Production airtime/rate limiting beyond payload, fragment and in-memory queue caps.
- Cross-route, hardware-verified deduplication.
- Delivery, range, latency or battery guarantees.

The core policy explicitly suppresses bulk CRDT sync and chat-history catch-up on constrained routes.

## Debug prerequisites

For a two-phone encrypted group-chat test:

- Two Android phones with compatible OFFBEAT builds.
- One supported, configured Meshtastic radio per phone.
- Matching legal region, modem and Meshtastic channel settings across the radios.
- Bluetooth permissions and Bluetooth powered on.
- Both OFFBEAT devices inside the same festival and joined to the same OFFBEAT group.
- Both apps open during the test.

The OFFBEAT group key and Meshtastic channel key are different secrets. Both layers must be configured correctly.

## Test-rig flow

This is a diagnostic flow, not a promise of normal use:

1. On each phone, open `YOU`, then `MESHTASTIC TEST RIG`.
2. Scan and select that phone's radio.
3. On the receiving phone, enter the current festival context and use `LISTEN + APPLY`.
4. While its timed listener is active, enter the shared group ID and a short message on the sending phone, then use `SEND GROUP CHAT`.
5. Treat `applied_group_chats` and appearance in local Social chat as evidence for that test only.

A successful send report confirms a Bluetooth write to the selected radio. It does not prove LoRa delivery. A local chat row on the sender also does not prove remote receipt.

## Offline behavior

The debug path does not need internet once both OFFBEAT groups and Meshtastic radios are configured. It still requires the phone-to-radio Bluetooth link and an active timed listener on the receiver.

There is no background receive service or durable radio mailbox in OFFBEAT. Messages sent outside the listener window may not enter the receiving OFFBEAT database. Meshtastic's own optional caching or Store & Forward behavior must not be treated as an OFFBEAT bulk-sync guarantee.

## Privacy and trust

Compact group chat is encrypted with the OFFBEAT group key before it enters the Meshtastic payload. A short keyed topic tag avoids broadcasting the raw group ID. Someone without the group key should not be able to decrypt the chat body.

Limits remain:

- Meshtastic routing headers and radio activity are observable.
- Repeated short topic tags and traffic timing can expose patterns.
- Anyone with the OFFBEAT invite key can decrypt that group's frames.
- Anyone with the Meshtastic channel key can participate at the radio-channel layer.
- Debug-applied group messages are marked unverified in the local chat trust model.

## Airtime, loss and duplicates

OFFBEAT uses a conservative 228-byte private application packet budget, a maximum of four fragments, and a default hop value of 3 in its compact frame. The current group-chat sender does not request a Meshtastic acknowledgment.

LoRa airtime is shared and constrained. Fragments may be delayed, lost, duplicated or arrive incomplete. The core frame layer includes message IDs and an in-memory dedupe component, but the current test rig is not proof of durable end-to-end deduplication across every OFFBEAT route.

Keep tests short. Do not send large payloads or repeated probes to compensate for uncertainty.

## Safety

Meshtastic is not an emergency service, and OFFBEAT's test rig adds more experimental layers. Never rely on it as the only way to request help. A send counter, relayed packet or decoded frame is not proof that a person read the message.

## Constraints

- The current path is Android-only, foreground, timed and manually operated.
- Only the compact encrypted group-chat proof is applied to a normal local resource.
- Full iroh, CRDT snapshots, lineup sync and bulk history are unavailable over LoRa.
- There is no restart-safe queue, persistent listener, measured rate limiter or delivery guarantee.
- Hardware and cross-route field validation remain incomplete.

## Troubleshooting

### No radios appear

Confirm Android Bluetooth permissions, radio power and Meshtastic BLE availability. Another phone may already hold the radio's single client connection.

### Connection or service discovery fails

Use the official Meshtastic app to confirm the phone can connect to that radio, then disconnect the official app before retrying OFFBEAT. Do not change legal radio settings merely to satisfy the test rig.

### The sender reports fragments but the receiver sees nothing

Confirm matching Meshtastic region, modem and channel settings, keep the receiver's listener active, move radios closer for a controlled test, and verify each phone selected its own radio.

### A frame arrives but is ignored

Confirm both OFFBEAT devices joined the same group under the same festival. An unknown keyed topic tag is deliberately ignored.

### A long message is rejected

Shorten it, especially if it contains emoji or other multi-byte characters.

Read [Meshtastic app and protocol](wiki:meshtastic.app-and-protocol) before configuring radios, and [Groups](wiki:offbeat.groups) for group-key limits.
