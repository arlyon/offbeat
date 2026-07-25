import { ed25519 } from "@noble/curves/ed25519.js";

const FESTIVAL_UPDATE_DOMAIN = new TextEncoder().encode("offbeat/festival-update/v1\0");

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

/** Build the cross-language canonical bytes signed for festival state. */
export function festivalUpdateSigningPayload(
	docId: string,
	kind: number,
	authoritySeq: bigint,
	update: Uint8Array,
): Uint8Array {
	if (kind !== 1 && kind !== 2) {
		throw new Error(`invalid festival update kind ${kind}`);
	}
	const docBytes = new TextEncoder().encode(docId);
	if (docBytes.length > 0xffff) throw new Error("festival document ID is too long");
	if (update.length > 0xffff_ffff) throw new Error("festival update is too large");

	const result = new Uint8Array(
		FESTIVAL_UPDATE_DOMAIN.length + 2 + docBytes.length + 1 + 8 + 4 + update.length,
	);
	let offset = 0;
	result.set(FESTIVAL_UPDATE_DOMAIN, offset);
	offset += FESTIVAL_UPDATE_DOMAIN.length;
	const view = new DataView(result.buffer);
	view.setUint16(offset, docBytes.length, false);
	offset += 2;
	result.set(docBytes, offset);
	offset += docBytes.length;
	view.setUint8(offset, kind);
	offset += 1;
	view.setBigUint64(offset, authoritySeq, false);
	offset += 8;
	view.setUint32(offset, update.length, false);
	offset += 4;
	result.set(update, offset);
	return result;
}

export async function signFestivalUpdate(
	secretKey: Uint8Array,
	docId: string,
	kind: number,
	authoritySeq: bigint,
	update: Uint8Array,
): Promise<Uint8Array> {
	return sign(secretKey, festivalUpdateSigningPayload(docId, kind, authoritySeq, update));
}
