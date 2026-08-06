const RELAY_AUTH_DOMAIN = new TextEncoder().encode("offbeat/relay-auth/v1\0");
const textEncoder = new TextEncoder();

function appendLengthPrefixed(parts: Uint8Array[], value: Uint8Array): void {
	if (value.byteLength > 0xffff) throw new Error("relay auth field is too large");
	const length = new Uint8Array(2);
	new DataView(length.buffer).setUint16(0, value.byteLength, false);
	parts.push(length, value);
}

/** Canonical proof of key possession for one FestivalDO socket challenge. */
export function relayAuthSigningPayload(
	festivalId: string,
	challenge: Uint8Array,
	publicKey: Uint8Array,
): Uint8Array {
	if (festivalId.length === 0) throw new Error("festival ID is required");
	if (challenge.byteLength !== 32) throw new Error("relay auth challenge must be 32 bytes");
	if (publicKey.byteLength !== 32) throw new Error("relay auth public key must be 32 bytes");
	const parts: Uint8Array[] = [RELAY_AUTH_DOMAIN];
	appendLengthPrefixed(parts, textEncoder.encode(festivalId));
	appendLengthPrefixed(parts, challenge);
	appendLengthPrefixed(parts, publicKey);
	const size = parts.reduce((total, part) => total + part.byteLength, 0);
	const payload = new Uint8Array(size);
	let offset = 0;
	for (const part of parts) {
		payload.set(part, offset);
		offset += part.byteLength;
	}
	return payload;
}
