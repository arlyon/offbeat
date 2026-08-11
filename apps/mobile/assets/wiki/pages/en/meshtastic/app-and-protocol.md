---
{
  "schemaVersion": 1,
  "id": "meshtastic.app-and-protocol",
  "locale": "en",
  "title": "Meshtastic app and protocol",
  "summary": "Understand Meshtastic radios, local phone links, LoRa channels, privacy, reliability, and safe off-grid use.",
  "category": "meshtastic",
  "countryCodes": [],
  "aliases": ["LoRa mesh", "Meshtastic radio", "Meshtastic channel", "PSK", "MQTT"],
  "tags": ["LoRa", "Bluetooth", "radio", "offline messaging", "privacy"],
  "generatedRefs": [],
  "priority": "high",
  "order": 800,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Introduction",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/introduction.mdx"
    },
    {
      "title": "Overview",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/index.mdx"
    },
    {
      "title": "Mesh algorithm",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/mesh-alg.mdx"
    },
    {
      "title": "LoRa configuration",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/lora.mdx"
    },
    {
      "title": "Channel configuration",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/channels.mdx"
    },
    {
      "title": "Encryption",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/encryption/index.mdx"
    },
    {
      "title": "Messages and channels",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/android/user/messages-and-channels.md"
    },
    {
      "title": "Bluetooth configuration",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/bluetooth.mdx"
    },
    {
      "title": "Network configuration",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/network.mdx"
    },
    {
      "title": "Device roles",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/device.mdx"
    },
    {
      "title": "Store and Forward module",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/module/store-and-forward-module.mdx"
    },
    {
      "title": "MQTT module",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/module/mqtt.mdx"
    },
    {
      "title": "Legal",
      "publisher": "Meshtastic",
      "url": "https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/legal/index.mdx"
    },
    {
      "title": "OFFBEAT Meshtastic implementation status",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/docs/meshtastic-implementation-plan.md"
    }
  ]
}
---
# Meshtastic app and protocol

Meshtastic provides best-effort, low-bandwidth communication through supported LoRa radios. Radio nodes can exchange and relay packets without mobile service, internet or a dedicated router. A phone or computer is normally a client connected locally to one radio.

> **Safety:** Meshtastic is not a guaranteed emergency service. Range, delivery, relay availability and response are never assured. Keep another appropriate way to get help whenever possible.

## Prerequisites

- A radio listed as supported by Meshtastic and running compatible firmware.
- A charged radio, phone and realistic backup-power plan.
- The correct legal region for the place where the radio is being used. A node with region set to `UNSET` does not transmit.
- Matching region and modem settings across nodes that need to communicate.
- For a private logical channel, the same channel name and randomly generated key on each member's node.

Follow local radio law. Do not override duty cycle, transmit power or frequency limits to work around poor reception.

## Phone link versus LoRa link

These are separate connections:

1. The client app sends data to the nearby radio over Bluetooth Low Energy, data-capable USB/serial, or a supported local TCP network connection.
2. The radio sends the packet over LoRa to compatible nodes.
3. Other nodes may relay it while its hop limit permits.

Losing Bluetooth does not prove that the radio mesh has stopped. Seeing a working LoRa node does not prove that the phone is still connected to it.

Bluetooth pairing may use a random PIN, fixed PIN or no PIN. The documented default fixed PIN is `123456`; change it because it is publicly known. A Meshtastic radio normally serves one phone or user connection at a time.

Some ESP32 nodes can join an existing 2.4 GHz Wi-Fi network for local TCP access. Official firmware does not provide Wi-Fi SoftAP mode, and enabling Wi-Fi on ESP32 disables Bluetooth. This Wi-Fi link is not the LoRa mesh.

## Radio and channel setup

A physical LoRa mesh needs compatible frequency, bandwidth and spreading-factor settings. In normal use this means matching the region and modem preset. The default `LONG_FAST` preset is a compromise, not a range promise. Slower presets consume more airtime and can reduce mesh capacity.

A node supports one primary and consecutive secondary logical channels, up to eight in total. Logical channels share the same radio settings but can have different names and pre-shared keys.

- The default and `simple` keys are public. They are not private communication.
- Private channels should use a random 256-bit key.
- Anyone with a channel key can decrypt captured channel payloads and can impersonate a channel sender under the shared-key channel model.
- Routing headers remain visible even when the payload is encrypted.
- Position sharing is configured per channel. Disable it or reduce precision when it is not needed.

Treat a channel QR code or URL like a password. Rotate the key if it is exposed.

## Hops, roles and delivery

Meshtastic uses managed flooding for broadcasts. The default hop limit is 3 and the maximum configurable value is 7. More hops do not guarantee greater useful range and can add congestion.

`CLIENT` is the normal role. Do not switch a handheld to `ROUTER` just to try to improve range. Infrastructure roles change power use, visibility, relay priority and client connectivity, and are intended for deliberate, well-placed installations.

Messages are small. The normal text composer and over-air application payload are around 200 bytes. Keep messages short and avoid repeated traffic.

Status meanings are limited:

- A broadcast relay acknowledgment means at least one node rebroadcast the packet. It does not mean every intended person received or read it.
- A direct-message acknowledgment can confirm delivery to the recipient node. It still does not prove a human read it.
- Timeouts, no route, exhausted retries, congestion, sleeping devices and regional duty-cycle limits can all prevent delivery.

For an important direct message, ask the recipient to reply with the key details.

## Offline behavior and history

Native LoRa messaging does not require internet. MQTT is an optional internet bridge and must not be treated as an offline path. Traffic that entered through MQTT is not proof of an all-LoRa route.

A disconnected node has only a small ordinary packet cache. The optional Store & Forward module has special hardware and configuration requirements, may return duplicates, and consumes substantial airtime when replaying history. Neither mechanism guarantees that every message waits for every recipient.

## Battery and placement

Runtime varies by hardware, traffic, GPS, screen, Wi-Fi, Bluetooth, role and retry rate. Transmitting usually costs more power than receiving. Charge before leaving, carry suitable backup power and verify the phone-to-radio link after changing power-saving settings.

Place a node clear of the body and large metal objects when practical. Higher placement and line of sight can help, but terrain, buildings, interference, antenna, hardware and legal settings still control the result. Record-setting links are not a usable range estimate.

## Privacy and trust

A channel key protects payload confidentiality, not anonymity. Node identifiers, routing metadata and radio activity remain observable. Position and telemetry can reveal sensitive information, especially on public channels or through MQTT.

Verify direct-message public keys before using them for sensitive information. Do not put private channel keys on unattended relay hardware unless that risk is acceptable.

## Constraints

- Delivery and range are best effort and depend on terrain, hardware, configuration, congestion and available relays.
- Logical channel encryption does not hide routing metadata or guarantee sender identity.
- Ordinary cache and optional Store & Forward do not provide a durable mailbox for every recipient.
- MQTT requires an internet path and is not an offline substitute.
- Meshtastic has no guaranteed link to public emergency dispatch.

## Troubleshooting

1. Confirm each radio is powered and the phone is connected to its own radio.
2. Confirm the region is set correctly and all participating nodes use compatible modem settings.
3. Confirm the logical channel name and key match exactly.
4. Check battery level, antenna connection and placement.
5. Send one short test message and wait. Repeated retries increase congestion.
6. Distinguish a local Bluetooth or USB failure from a LoRa delivery failure.
7. If a message is urgent, use another communication method rather than repeatedly trusting an unknown Meshtastic state.

For OFFBEAT's separate experimental radio path, see [OFFBEAT over Meshtastic](wiki:meshtastic.offbeat-over-meshtastic).
