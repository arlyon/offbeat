/// Server configuration for the OFFBEAT app.
///
/// In production, these would come from build-time config or a .env file.
/// For now, defaults point at the local wrangler dev server.

const String mainDoBaseUrl = String.fromEnvironment(
  'OFFBEAT_SERVER_URL',
  defaultValue: 'http://localhost:8787',
);

/// MainDO's Ed25519 public key (hex) — the trust anchor for attestation
/// verification. In production this is hardcoded from the deployed MainDO.
/// In dev, fetch it from GET /auth/public-key on first run.
const String mainDoPublicKeyHex = String.fromEnvironment(
  'OFFBEAT_SERVER_PUBKEY',
  defaultValue: '', // empty = fetch from server in dev
);
