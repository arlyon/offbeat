---
{
  "schemaVersion": 1,
  "id": "offbeat.groups",
  "locale": "en",
  "title": "OFFBEAT groups",
  "summary": "Create or join an encrypted festival group, share presence and picks, use chat, and understand key and recovery limits.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["crew", "group invite", "group QR", "check-in", "group chat"],
  "tags": ["groups", "privacy", "encryption", "presence", "chat"],
  "generatedRefs": [],
  "priority": "high",
  "order": 830,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Group domain manager",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/groups.rs"
    },
    {
      "title": "Mobile group lifecycle and retries",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/mod.rs"
    },
    {
      "title": "Social groups screen",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/screens/social/social_screen.dart"
    },
    {
      "title": "Group presence interpretation",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/data/group_presence.dart"
    },
    {
      "title": "Group encryption",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/crypto.rs"
    }
  ]
}
---
# OFFBEAT groups

Groups let a festival crew share a private schedule signal, check-in presence and group chat. Group state is a Yrs CRDT encrypted with a random group key.

## Current support

**Available in the normal mobile UI:** create a festival-scoped group, switch between groups, invite by code or QR, join by code or camera scan, view members and their check-ins, share personal picks, send group chat, and leave.

**Available with delivery limits:** group-state changes persist locally and retry relay delivery. Group chat is stored locally and sent through active routes, but its outbound delivery is not restart-safe and has no read receipt.

**Not available in the normal UI:** group administrators, moderation, member removal by another member, key rotation, forgotten-key recovery, or map-pin management. Core pin data exists, but that is not proof of an attendee UI.

## Prerequisites

- Select the festival the group belongs to.
- Create a group or receive its full OFFBEAT invite.
- Keep a safe copy of the invite if rejoining later may be necessary.
- For live sharing, at least one relay or compatible peer route must eventually be available.

An invite has the form of an OFFBEAT group link and contains the festival scope, derived group ID and secret group key. The app verifies that the key derives the stated group ID and that the invite belongs to the current festival.

## Create, invite and join

Creating a group generates a new 32-byte key, stores the group locally, adds the creator as a member, registers group state and chat resources, and shows the invite sheet.

Use `INVITE` to display a shareable code and QR. A recipient can paste the code or scan the QR from the same festival's Social screen. Joining saves the key locally, adds the member and begins catch-up.

Share the invite only with intended members. It is not a harmless identifier.

## Members and check-ins

A member can check in to a stage, campsite or custom location, or clear the check-in. OFFBEAT stores one current check-in and mirrors it to each joined group for that festival.

Check-ins expire after four hours and then display as stale. Stale does not mean the person has left the location. It means OFFBEAT has not accepted a fresh check-in within the freshness window. `NO CHECK-IN YET` and `OFFLINE` are status labels, not proof of a person's real location or safety.

Do not use group presence for emergencies or welfare guarantees. Contact the person or venue staff directly when safety matters.

## Shared schedules

When you star or unstar a set, OFFBEAT keeps the personal choice in local SQLite and reconciles your complete pick list into each group for that festival. Other members can then appear as supporters of that set.

Group-visible picks do not become another member's personal schedule. See [Likes and personal schedule](wiki:offbeat.likes-and-personal-schedule).

## Group chat

Messages are encrypted with the group key and stored in the local chat database. The sender sees a locally accepted message immediately. Active gossip and relay routes are attempted, but there is no guaranteed delivery, durable remote mailbox or read receipt.

If the sender was offline, closing the app before another route accepts the message can leave the only copy on the sender. For important coordination, ask for a reply and repeat the essential information through another channel when needed.

## Offline behavior

With the group key and cached state, you can open the group, read stored members, presence, shared picks and chat without internet. Local CRDT changes can be made while partitioned and may merge later.

Group-state operations such as membership, check-ins and shared picks are put in a persistent encrypted outbound queue for relay retry. Group chat does not yet have the same restart-safe retry behavior.

Joining from an invite can create local membership while offline, but no existing group state or history can arrive until a matching member or relay becomes reachable.

## Privacy and trust

Group state and chat use AES-256-GCM with the group key. Relay infrastructure can carry ciphertext without the plaintext or key. Unrelated peers without the key cannot decrypt it.

This design has important limits:

- Anyone with the invite key can decrypt group resources and act as a member from a compatible client.
- There is no current moderation or revocation mechanism.
- Encryption does not hide all traffic metadata, timing or packet size.
- Display names and check-ins are member-provided data, not independently verified location.
- Losing the only invite or key has no recovery flow.

## Leaving and recovery

Leaving removes your member and shared-star entries, queues an encrypted leave update, then deletes the local group key, cached group document and local group chat. Personal stars are kept.

Other members may continue to show your old entry until they receive the leave update. Once the local key is removed, OFFBEAT cannot decrypt that group's state. Rejoining requires a valid invite. There is no history or key-recovery promise.

## Constraints

- Group delivery and convergence are not immediate or guaranteed.
- Normal group chat has no restart-safe outbound queue or read receipt.
- Invites grant key possession and cannot currently be revoked or rotated.
- There is no administrator, moderation, member-removal or key-recovery flow.
- Check-ins are self-reported and become stale after four hours.

## Troubleshooting

### An invite is rejected

Confirm you are inside the intended festival, copied the whole code, and did not add spaces. A group invite for another festival is deliberately rejected.

### The group name or members are missing

The local join may be complete before catch-up. Keep the group selected, check active routes, and wait for another group member or relay. Do not repeatedly create replacement groups.

### A check-in is stale

Ask the member to refresh it. Stale status is expected after four hours and does not diagnose connectivity or personal safety by itself.

### A chat message is only visible to the sender

That proves local persistence only. Keep both apps open while a route is available, check the connection drawer, and ask the recipient to acknowledge. The current app cannot promise automatic resend after a restart.

### You left accidentally

Obtain a new copy of the existing invite from a current member. OFFBEAT has no recovery key and does not restore the deleted local private history.

For route behavior, see [P2P syncing](wiki:offbeat.p2p-syncing).
