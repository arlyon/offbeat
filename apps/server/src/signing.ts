import { ed25519 } from "@noble/curves/ed25519.js";

export function generateKeypair(): {
	publicKey: Uint8Array;
	secretKey: Uint8Array;
} {
	const { secretKey, publicKey } = ed25519.keygen();
	return { publicKey, secretKey };
}

export async function sign(secretKey: Uint8Array, data: Uint8Array): Promise<Uint8Array> {
	return ed25519.sign(data, secretKey);
}

export async function verify(
	publicKey: Uint8Array,
	data: Uint8Array,
	signature: Uint8Array,
): Promise<boolean> {
	return ed25519.verify(signature, data, publicKey);
}
