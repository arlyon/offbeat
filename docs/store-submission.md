# Store submission details

This document records the intended public listing and questionnaire answers. It
is preparation only; it does not authorise a deployment, metadata upload, build
upload, review submission, or public release.

## Positioning

- Name: **Offbeat Festival Companion**
- Subtitle: **Lineups, groups, offline**
- Apple primary category: **Music**
- Apple secondary category: **Social Networking**
- Google Play category: **Events**
- Google Play tags, where available: **Music**, **Live Events**, **Social**
- Marketing URL: <https://offbeat.arlyon.dev>
- Support URL: <https://offbeat.arlyon.dev/support>
- Privacy URL: <https://offbeat.arlyon.dev/privacy>
- Initial availability excludes France.

Localized App Store metadata is in `apps/mobile/fastlane/metadata/ios`. Google
Play metadata is in `apps/mobile/fastlane/metadata/android`. Both contain en-GB
and en-US variants. Store screenshots are under `apps/mobile/fastlane/screenshots`
and the platform-specific Android image directories.

The app icon displayed by TestFlight and the stores comes from the signed binary.
The Google Play 512 px icon and 1024×500 feature graphic are also present in each
Play locale.

## Apple privacy labels

Proposed answers based on the current implementation:

- Data used to track users: **No**
- Third-party advertising: **No**
- Developer advertising or marketing: **No**
- Analytics: **No analytics SDK is included**
- App functionality:
  - Name: optional display name or alias
  - User ID: pseudonymous public identity and passkey credential identifier
  - User content: group and public chat messages, pins, and manual check-ins
  - Product interaction: starred sets, group membership, and schedule state
  - Other data: peer and protocol identifiers required for synchronisation
- Precise or coarse device location: **Not collected**
- Camera images: **Not collected**; QR frames are processed on device
- Diagnostics: **Not collected by an analytics or crash-reporting service**
- Data linked to identity: treat the app-functionality data above as linked to
  the user's pseudonymous Offbeat identity

Private group payloads are encrypted before relay. Public festival chat and
festival state can be replicated to participating peers. All declared purposes
are app functionality.

## Google Play data safety

Proposed answers based on the current implementation:

- Data shared for advertising or tracking: **No**
- Data collected for app functionality:
  - Optional name or alias
  - Pseudonymous user and device/peer identifiers
  - Messages and other user-generated content
  - Starred sets, groups, and manual check-ins
- Location permission or GPS location: **No**
- Camera data retained or transmitted: **No**
- Data encrypted in transit: **Yes**
- Users can clear private local data by leaving groups, logging out, clearing app
  storage, or uninstalling
- Requests concerning server-held identifiers can be made through the support
  page; users should not post secrets in public issues

Google Play category and tags must be selected in Play Console because `supply`
does not manage them reliably.

## Review notes

The reviewer can register a first-party passkey on the test device; no social
login or pre-created demo account is required. Festival schedules, starring,
clash detection, and offline browsing can be reviewed on one device. Group and
peer-to-peer synchronisation are best exercised with two devices. Camera access
is requested only when the QR scanner is opened. Check-ins are selected manually
and do not use device location services.

## Outstanding owner decision

Export-compliance classification remains owned by Alex. Do not submit a build or
answer Apple's encryption questionnaire using the standard-encryption-only
classification unless the shipped binary has first been verified to qualify.
