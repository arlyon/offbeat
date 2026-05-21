/// Server configuration for the OFFBEAT app.
///
/// In production, these would come from build-time config or a .env file.
/// For local dev on a physical device, pass the server's LAN IP:
///   flutter run --dart-define=OFFBEAT_SERVER_URL=http://192.168.1.100:8787
library;

const String mainDoBaseUrl = String.fromEnvironment(
  'OFFBEAT_SERVER_URL',
  defaultValue: 'http://localhost:8787',
);

/// The WebAuthn RP ID — must match the domain serving assetlinks.json.
/// For local dev: "localhost". For production: "offbeat.app".
const String rpId = String.fromEnvironment(
  'OFFBEAT_RP_ID',
  defaultValue: 'localhost',
);

/// MainDO's Ed25519 public key (hex) — the trust anchor for attestation
/// verification. In production this is hardcoded from the deployed MainDO.
/// In dev, fetch it from GET /auth/public-key on first run.
const String mainDoPublicKeyHex = String.fromEnvironment(
  'OFFBEAT_SERVER_PUBKEY',
  defaultValue: '', // empty = fetch from server in dev
);
