# Meshtastic offline-guide source report

**Research date:** 2026-08-11

**Source policy:** Official Meshtastic documentation, source, and specification repositories only. No community posts, vendor marketing, or third-party summaries were used for factual claims.

**Documentation snapshot:** [`meshtastic/meshtastic` commit `99bef750b6a962a08d7a9ab18ed3736f0d9d664d`](https://github.com/meshtastic/meshtastic/commit/99bef750b6a962a08d7a9ab18ed3736f0d9d664d) (2026-08-10). Links below use commit-pinned official source pages so the evidence does not silently change.

## Scope boundary

This report describes Meshtastic itself. It is source material for cautious user-facing documentation. It does **not** establish, imply, or instruct authors to claim that OFFBEAT currently integrates with Meshtastic. Any OFFBEAT interoperability claim would need separate implementation and validation evidence.

## Executive summary

Meshtastic is an open-source, community-driven system that turns supported LoRa radios into a decentralized, long-range, low-bandwidth mesh for off-grid text and optional location/data exchange. LoRa communication between nodes does not require a phone, cellular service, internet, or a dedicated router. A phone or computer is normally a **client** connected locally to one radio by Bluetooth, serial/USB, or local IP networking; that local link is distinct from the LoRa mesh. [Introduction][intro] [Overview][overview]

The safe user-facing framing is: **useful off-grid messaging, not guaranteed delivery or an emergency service**. Official documentation calls the project only “mostly” stable, calls related hardware experimental because Meshtastic does not manufacture it and it has not been tested by bodies such as UL or FCC, documents ordinary no-route/time-out/retry-exhaustion conditions, and limits reliable zero-hop retries to three. [Legal][legal] [Mesh algorithm][mesh-algo] [Android message states][messages]

## 1. What the system consists of

### Radio node versus client app

- A **node** is a device running Meshtastic firmware with a supported LoRa radio. Nodes exchange and relay packets over LoRa. Mesh communication itself does not require a phone. [Introduction][intro]
- An **app/client** connects to a node to configure it and send or receive data through it. A message composed in a client travels over Bluetooth, Wi-Fi/Ethernet, or serial to the node; only then does the node transmit it over LoRa. [Overview][overview]
- A radio can be paired with a single phone/user connection at a time. Therefore, do not describe one node as a multi-user Wi-Fi-style access point. [Introduction][intro]
- Official client surfaces include the Android/desktop app, Apple apps, Web Client, and Python CLI. Exact transport support depends on client, browser, operating system, and node hardware. [Initial configuration][initial-config] [Web Client][web-client] [Python CLI][python-cli]

### Supported radio hardware

The official hardware catalog is broad and changes over time, so user-facing material should link to it rather than freeze an exhaustive compatibility list. The current catalog includes devices based on ESP32, nRF52, RP2040, and Linux/`meshtasticd`, with LoRa transceivers including SX126x, LR11xx, SX127x, and SX1280 variants. The project strongly recommends newer SX126x or LR11xx radios over SX127x for performance and compatibility. [Supported devices][devices]

Useful platform distinctions from the official guide:

- **nRF52:** lower power than ESP32 and generally preferred for solar or handheld use; Bluetooth but normally no Wi-Fi. [Getting started][getting-started] [Supported devices][devices]
- **ESP32:** usually lower cost, higher power consumption, and appropriate when Wi-Fi, more RAM, or fixed power is needed. [Supported devices][devices]
- **RP2040/RP2350:** supported on selected boards, but transport capabilities vary; for example, official docs say Pico W Bluetooth is not currently supported by Meshtastic. [Getting started][getting-started]

All listed supported boards can participate together when their LoRa radio settings match; matching the board brand is not required. [Supported devices][devices] [Overview][overview]

## 2. Phone/computer-to-node links are local links

### Bluetooth Low Energy

Bluetooth is the normal phone-to-node path. Pairing can use a runtime random PIN, a fixed PIN, or no PIN. A screen-equipped first boot normally selects a random PIN; a screenless first boot normally selects fixed-PIN mode. The documented default fixed PIN is `123456`, and the official docs strongly recommend changing it because leaving it in place is a security risk. [Bluetooth configuration][bluetooth]

Bluetooth range or a dropped phone connection does not define LoRa range. Once a message has reached the node, the radio mesh is a separate link. Conversely, a functioning LoRa node does not guarantee that the phone remains connected to it. [Overview][overview]

### USB/serial

Serial over a data-capable USB cable is a wired client-to-node option. Android USB requires OTG support; desktop clients and the Python CLI can also use serial ports. A charging-only cable is insufficient. [Android connections][connections] [Getting started][getting-started] [Python CLI][python-cli]

### Wi-Fi/Ethernet and TCP

- Some nodes support a TCP/IP client connection over a local network. This is still a client-to-node path; internet access is not inherently required for local TCP. [Android connections][connections]
- ESP32 firmware joins an existing 2.4 GHz Wi-Fi network as a client; official firmware does not support Wi-Fi SoftAP mode. [Network configuration][network]
- On ESP32, enabling Wi-Fi disables Bluetooth; the docs say only one of those connection methods works at a time. Wi-Fi is disabled by default. [Network configuration][network] [Bluetooth configuration][bluetooth]
- Network discovery is local-subnet mDNS, while direct IP/hostname connection is also available. The documented default TCP port in the Android client is `4403`. [Android connections][connections]

Do not write “Meshtastic needs Wi-Fi” merely because a phone UI offers a Network transport. LoRa node-to-node operation and local Wi-Fi/TCP administration are different layers. [Getting started][getting-started]

## 3. LoRa mesh behavior

A radio mesh consists of nodes sharing compatible center frequency, bandwidth, and spreading factor. In normal configuration, nodes must match **region** and **modem preset** (or identical custom modem settings). A node configured differently will not participate in that radio mesh. [Overview][overview] [LoRa configuration][lora]

For broadcasts, Meshtastic uses managed flooding:

1. A receiving node rejects duplicate packet IDs.
2. If hop limit permits, a node decrements it and may rebroadcast.
3. Nodes briefly listen before relaying so that they can suppress redundant rebroadcasts; farther nodes are favored by SNR-based contention, while infrastructure roles can receive relay priority.
4. A packet at hop limit zero is not forwarded further. Maximum configurable hop limit is 7; default is 3, which official docs recommend for most uses. [Mesh algorithm][mesh-algo] [LoRa configuration][lora]

Since firmware 2.6, direct messages can learn a next hop after a successful exchange, reducing indiscriminate relaying. If that route stops working, the final retry falls back to managed flooding. This optimization does not turn the network into a guaranteed routed service. [Mesh algorithm][mesh-algo]

Nodes also emit background NodeInfo, position, and telemetry traffic. Firmware increases intervals on larger or busier meshes and applies duty-cycle, channel-utilization, and airtime throttling. [Mesh algorithm][mesh-algo]

## 4. Channels and pre-shared keys

“Channel” has two meanings that user-facing text must keep separate:

- **Radio frequency slot/modem configuration:** shared physical LoRa settings for the whole node.
- **Logical Meshtastic channel:** one of up to eight message groups, each with a name, key, and settings. All logical channels on a node use the same modem settings. [Channel configuration][channels]

A node can have one primary channel and consecutive secondary channels, up to eight total. Nodes need the same channel name and PSK to decrypt and display that channel’s content. Radios on the same physical mesh may still receive and relay packets they cannot decrypt, depending on device/rebroadcast role. [Overview][overview] [Channel configuration][channels]

PSK facts that should be explicit:

- Channel PSKs can be absent, 16 bytes (AES-128), or 32 bytes (AES-256). [Channel configuration][channels]
- The default key (`AQ==`) and `simple` keys are publicly known and are suitable for testing/public channels, **not private communication**. Official docs direct users to generate a random 256-bit key for private communication. [Channel configuration][channels]
- Group/channel payloads use shared-key encryption, but routing headers remain unencrypted. The official security page also documents no integrity verification for channel messages and warns that someone with the channel key can impersonate a channel sender. Direct messages between current compatible nodes use public-key encryption/signatures, but users should still verify recipient public keys for sensitive information. [Encryption][encryption]
- Possession of a private channel key allows decryption of captured channel traffic; official guidance advises rotating keys and not installing private keys on physically unattended relay nodes. [Encryption][encryption]

Accordingly, avoid saying “private channel means anonymous” or “all packet data is hidden.” Payload confidentiality, routing-metadata exposure, key custody, sender authentication, and location sharing are separate concerns. [Encryption][encryption]

## 5. Device roles

`CLIENT` is the normal general-purpose role: it can connect to an app or operate standalone and relays when another node has not already done so. Most users should remain on this role. [Device roles][device]

Other current roles specialize behavior:

- `CLIENT_MUTE`: participates but does not forward others’ packets, reducing network load.
- `CLIENT_HIDDEN`: transmits only as needed for power saving/low visibility and performs local-only retransmission.
- `CLIENT_BASE`: personal base station that prioritizes packets to/from favorited nodes.
- `TRACKER`: prioritizes GPS position packets.
- `LOST_AND_FOUND`: regularly broadcasts location as a message for recovery.
- `SENSOR`: prioritizes telemetry.
- `TAK` / `TAK_TRACKER`: optimize traffic for TAK use.
- `ROUTER`: always-on, high-power infrastructure role that prioritizes retransmission and is visible in node lists.
- `ROUTER_LATE`: always retransmits, but after other modes, for local dead-spot coverage.
- `REPEATER`: minimal-overhead infrastructure relay, hidden from node lists; official docs mark it deprecated as of firmware 2.7.11. [Device roles][device]

Roles change power, client connectivity, visibility, sleep, and retransmission behavior. Do not recommend `ROUTER` merely to “improve range” on a handheld: the official comparison marks it high power and disables normal local client links under its default ESP32 power-saving behavior. Infrastructure roles should be deliberate and strategically placed. [Device roles][device]

## 6. Range, capacity, and airtime constraints

There is no single dependable “Meshtastic range.” Official radio documentation describes a trade-off, not a guarantee: higher spreading factor increases link budget/range but doubles airtime at each step; more bandwidth increases speed but reduces link budget; more coding redundancy can improve resistance to noise while reducing data rate. Actual results also depend on hardware and configuration, and the official site-planner documentation models terrain/path loss and obstacles as reliability factors. [Radio settings][radio-settings] [Site Planner][site-planner]

The default `LONG_FAST` preset is intended as a speed/range compromise. `VERY_LONG_SLOW` has the highest airtime and longest nominal range but is explicitly “not recommended for regular usage” because it forms meshes poorly and is unreliable. `SHORT_TURBO` is fastest/shortest-range and is not legal in every region because of its bandwidth. [LoRa configuration][lora]

Capacity is small compared with cellular or Wi-Fi:

- The over-air application payload is around 200 bytes after headers; the Android composer caps text at 200 bytes. [Overview][overview] [Android message states][messages]
- Every relay, retry, beacon, position update, telemetry packet, and MQTT downlink consumes shared airtime. Heavy volume may be throttled. [Mesh algorithm][mesh-algo] [Android message states][messages]
- Nodes use channel-activity detection and randomized contention, but collisions and congestion are still possible. [Mesh algorithm][mesh-algo]
- Regional duty-cycle rules can stop transmission until the rolling window permits it again; EU 433 and EU 868 are documented with a 10% rolling-hour limit. Overriding the firmware limit can violate regulations. [Region table][regions] [LoRa configuration][lora]

User-facing guidance should favor short, essential messages and avoid promising a distance based on record-setting links.

## 7. Store-and-forward is optional and limited

A node disconnected from its client retains only a small default cache of roughly 30 packets; when full, the oldest entries are replaced by newly received text messages. This is not a durable mailbox guarantee. [Overview][overview]

The optional Store & Forward module can retain more text history, but it has important limits:

- A server must be an ESP32 device with onboard PSRAM and is intended to remain continuously online.
- If the server misses a message, stored-history reliability is reduced.
- It returns messages in the requested time window up to a configured maximum; it does not know exactly which messages a client missed, so duplicates are possible.
- Replaying history consumes substantial LoRa airtime and can temporarily burden the mesh.
- History requests over LoRa are unavailable on the default public channel.
- Default server settings are 25 returned messages, a 240-minute return window, and storage sized to about 11,000 records when available PSRAM permits. [Store & Forward][store-forward]

Therefore, “offline” should not be written as “every message waits until every recipient reconnects.” Ordinary node cache, app-local history, and the optional Store & Forward server are different mechanisms. [Overview][overview] [Store & Forward][store-forward]

## 8. MQTT and the internet are optional bridges

Native LoRa mesh communication does not require internet. MQTT is an **optional gateway** that forwards selected mesh packets through an MQTT broker when a node has Wi-Fi/Ethernet internet access or uses a supported client’s internet connection as a proxy. Channels must separately enable uplink and/or downlink. [MQTT module][mqtt-module]

Consequences for wording:

- A packet received through MQTT is explicitly marked as having entered via MQTT; it is not evidence of an all-LoRa path. [Overview][overview]
- Public MQTT traffic is subject to server-side filtering. The official public server currently applies a zero-hop downlink policy: directly connected LoRa nodes can receive public MQTT data, but it is not propagated farther through the local mesh. [MQTT integration][mqtt-integration]
- Private brokers do not automatically apply the public server’s protections and can flood a mesh if configured carelessly. [MQTT integration][mqtt-integration]
- MQTT encryption is separately configurable. If MQTT encryption is disabled, packets are sent to the broker unencrypted even if the uplink channel has a key; JSON MQTT packets are also unencrypted. [MQTT module][mqtt-module]

A UI should distinguish **LoRa-only**, **local Bluetooth/USB/LAN connection**, and **internet/MQTT gateway** states rather than collapsing all three into “online/offline.”

## 9. Location and privacy

Position is optional and may come from a node’s GPS or the paired phone. Smart broadcast can increase update frequency when moving; the default periodic position interval is 15 minutes, with a default smart-broadcast minimum movement of 100 m and minimum interval of 30 seconds. More frequent updates consume more airtime and battery. [Position configuration][position]

Privacy controls and residual exposure:

- Position precision is per logical channel: `0` means never send location on that channel, `32` means full precision, and intermediate values deliberately reduce precision. [Channel configuration][channels]
- Periodic position and telemetry are normally sent on the primary channel, so primary-channel key choice matters. [Channel configuration][channels] [Encryption][encryption]
- The public MQTT server filters default-key positions to 10–16 precision bits, but that server-side filter is not a substitute for configuring the sender appropriately. [MQTT integration][mqtt-integration]
- Optional MQTT map reporting is unencrypted and includes node name/ID, position/altitude at configured precision, hardware, role, firmware, region, preset, primary channel name, and local-node count. [MQTT module][mqtt-module]
- `OK to MQTT = false` is only a polite request enforced by official firmware for certain known-key channels, not a cryptographic control. [LoRa configuration][lora]
- LoRa routing headers are unencrypted even when application payloads are encrypted. [Encryption][encryption]

User-facing defaults should minimize precision and disable unnecessary position/MQTT reporting. Do not imply that a PSK hides the existence, identifiers, or radio activity of a node.

## 10. Region and frequency configuration

A user must set `lora.region` for the node’s actual location. The default is `UNSET`; while unset, the node displays a warning and does not transmit. Region settings select legal frequency ranges, duty-cycle handling, and power limits. [LoRa configuration][lora] [Region table][regions]

Nodes also need matching region and modem settings to communicate. The primary channel name can influence the automatically selected frequency slot, so devices with different primary names may need an explicit common frequency slot. [LoRa configuration][lora] [Channel configuration][channels]

Safe documentation should tell users to select the official region code for where the radio is physically operated and comply with local law. It should not instruct users to override duty cycle, transmit power, or out-of-band frequency. The official docs reserve frequency override for advanced/licensed use and explicitly warn users to respect local law. [LoRa configuration][lora]

## 11. Battery and power considerations

Battery life is device- and workload-specific; do not publish a universal runtime. Official hardware guidance says nRF52 devices generally use less power than ESP32 devices. ESP32 may be suitable on fixed power or for shorter handheld runtime, while nRF52 is generally preferred for solar/handheld deployments. [Supported devices][devices]

Major runtime factors include:

- transmitting consumes substantially more power than receiving;
- baseline beacons and responses consume power even with no manually typed messages;
- GPS, Wi-Fi, Bluetooth, screens, background updates, modules, relay role, traffic volume, and retry rate alter consumption;
- infrastructure roles are marked high power, while mute/hidden roles use less;
- power-saving mode disables Bluetooth, serial, Wi-Fi, and the screen, so a sleeping radio may not be reachable from its local client. [Power measurement][power-measurement] [Power configuration][power] [Device roles][device]

The official power guide recommends measuring representative average consumption over time rather than calculating from an idle reading. For user-facing safety material, advise charging before departure, carrying an appropriate backup-power plan, and verifying the radio/phone link after power-saving changes; do not infer “excellent battery life” as a guaranteed number. [Power measurement][power-measurement]

## 12. Message reliability and acknowledgment semantics

Meshtastic improves best-effort radio delivery but does not make it certain:

- A reliable packet requests an ACK. If no ACK/NAK arrives before timeout, the local node retries up to three times and then generates a failure/NAK. [Mesh algorithm][mesh-algo]
- For a **broadcast**, hearing any Meshtastic node rebroadcast the packet creates an implicit ACK—even if that relay lacks the channel key. This means only that at least one node relayed it, not that every intended person received or read it. [Mesh algorithm][mesh-algo]
- For a **direct message**, the intended recipient can return an end-to-end ACK; the app distinguishes recipient delivery from “relayed, not confirmed.” [Mesh algorithm][mesh-algo] [Android message states][messages]
- Documented failures include no route, NAK, timeout, no radio interface, exhausted retransmissions, missing channel, oversized payload, sleeping/busy recipient, and regional duty-cycle limit. [Android message states][messages]

User-facing copy should preserve these distinctions: **queued**, **reached/was relayed by the mesh**, **recipient acknowledged**, and **failed/unknown** are not interchangeable. A check mark should never be described as proof that a human read the message.

## 13. Emergency limitations and safe wording

### Official evidence

- The project describes itself as community-driven and volunteer-supported. [Introduction][intro]
- Official legal documentation calls functionality “mostly” stable, says the project is rapidly evolving, and says associated devices should be considered experimental because Meshtastic neither manufactures them nor has them tested by regulatory organizations such as UL or FCC. [Legal][legal]
- The protocol and app documentation explicitly expose ordinary delivery failures, finite retries, congestion/duty-cycle blocking, finite hop limits, and limited/non-guaranteed history. [Mesh algorithm][mesh-algo] [Android message states][messages] [Store & Forward][store-forward]
- No official source reviewed supplies a guaranteed-delivery SLA or a connection to public emergency-dispatch services.

### Editorial safety conclusion

The evidence supports this user-facing warning:

> Meshtastic can be a useful supplementary off-grid communication tool, but it is not a guaranteed emergency service. Do not rely on it as your only way to call for help. Range, relay availability, congestion, battery state, configuration, terrain, and radio regulations can delay or prevent delivery. For urgent messages, seek a direct-recipient acknowledgment and use another appropriate emergency method whenever available.

This paragraph is a conservative editorial inference from the official limitations above, not a quoted Meshtastic warranty or an OFFBEAT integration claim. Avoid claims such as “always works without internet,” “guaranteed rescue communications,” “SOS reaches emergency services,” or “delivered to mesh means the recipient got it.”

## Source index

All sources below are official Meshtastic repository pages pinned to the researched commit.

[intro]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/introduction.mdx
[overview]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/index.mdx
[mesh-algo]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/mesh-alg.mdx
[radio-settings]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/radio-settings.mdx
[encryption]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/about/overview/encryption/index.mdx
[devices]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/hardware/devices/index.mdx
[getting-started]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/getting-started/index.mdx
[initial-config]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/getting-started/initial-config.mdx
[bluetooth]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/bluetooth.mdx
[network]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/network.mdx
[channels]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/channels.mdx
[device]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/device.mdx
[lora]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/lora.mdx
[regions]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/blocks/_lora-regions.mdx
[position]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/position.mdx
[power]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/radio/power.mdx
[store-forward]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/module/store-and-forward-module.mdx
[mqtt-module]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/configuration/module/mqtt.mdx
[mqtt-integration]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/integrations/mqtt/index.mdx
[connections]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/android/user/connections.md
[messages]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/android/user/messages-and-channels.md
[web-client]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/web-client.mdx
[python-cli]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/python-cli/index.mdx
[site-planner]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/software/site-planner/index.mdx
[power-measurement]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/hardware/solar-powered/measure-device-power-consumption.mdx
[legal]: https://github.com/meshtastic/meshtastic/blob/99bef750b6a962a08d7a9ab18ed3736f0d9d664d/docs/legal/index.mdx
