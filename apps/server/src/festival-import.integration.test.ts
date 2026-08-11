import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { ed25519 } from "@noble/curves/ed25519.js";
import {
	FestivalUpdateKind,
	RelayClientMessageSchema,
	RelayServerMessageSchema,
} from "@offbeat/protocol";
import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import * as Y from "yjs";
import { festivalImportSigningPayload } from "./festival-import";
import { festivalUpdateSigningPayload } from "./signing";

const fixture = {
	name: "Community Festival 2027",
	locations: [
		{
			name: "Main Stage",
			events: [
				{
					name: "Artist One",
					start: "2027-06-12 12:00",
					end: "2027-06-12 13:00",
				},
			],
		},
	],
};

function bytesToHex(bytes: Uint8Array) {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function hexToBytes(hex: string) {
	return new Uint8Array(hex.match(/.{2}/g)?.map((byte) => Number.parseInt(byte, 16)) ?? []);
}

function bytesToBase64(bytes: Uint8Array) {
	let binary = "";
	for (const byte of bytes) binary += String.fromCharCode(byte);
	return btoa(binary);
}

function keypair() {
	const secretKey = ed25519.utils.randomSecretKey();
	return { secretKey, publicKey: bytesToHex(ed25519.getPublicKey(secretKey)) };
}

async function register(worker: Unstable_DevWorker, publicKey: string) {
	const begin = await worker.fetch("/auth/register/begin", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ userId: `import-${publicKey.slice(0, 8)}` }),
	});
	const { challenge } = (await begin.json()) as { challenge: string };
	const complete = await worker.fetch("/auth/register/complete", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ webauthnResponse: {}, challenge, ed25519PublicKey: publicKey }),
	});
	expect(complete.status).toBe(200);
	return (await complete.json()) as {
		attestation: { message: string; signature: string; issuer: string };
	};
}

function authHeaders(
	path: string,
	body: string,
	identity: ReturnType<typeof keypair>,
	attestation: { message: string; signature: string; issuer: string },
	overrides: { timestamp?: string; nonce?: string } = {},
) {
	const timestamp = overrides.timestamp ?? Math.floor(Date.now() / 1000).toString();
	const nonce = overrides.nonce ?? crypto.randomUUID().replace(/-/g, "");
	const payload = festivalImportSigningPayload("POST", path, timestamp, nonce, body);
	return {
		"Content-Type": "application/json",
		"CF-Connecting-IP": `2001:db8:${identity.publicKey.slice(0, 4)}::1`,
		"X-Attestation-Message": attestation.message,
		"X-Attestation-Signature": attestation.signature,
		"X-Attestation-Issuer": attestation.issuer,
		"X-Session-PublicKey": identity.publicKey,
		"X-Request-Timestamp": timestamp,
		"X-Request-Nonce": nonce,
		"X-Request-Signature": bytesToHex(ed25519.sign(new TextEncoder().encode(payload), identity.secretKey)),
	};
}

async function signedCheckpoint(festivalId: string) {
	const wsScheme = ["w", "s"].join("");
	const ws = new WebSocket(`${wsScheme}://${worker.address}:${worker.port}/festivals/${festivalId}/ws`);
	ws.binaryType = "arraybuffer";
	await new Promise<void>((resolve, reject) => {
		ws.onopen = () => resolve();
		ws.onerror = () => reject(new Error("WebSocket connection failed"));
	});
	const response = new Promise<ReturnType<typeof fromBinary<typeof RelayServerMessageSchema>>>(
		(resolve, reject) => {
			const timeout = setTimeout(() => reject(new Error("Checkpoint timeout")), 5_000);
			ws.addEventListener("message", (event) => {
				const message = fromBinary(
					RelayServerMessageSchema,
					new Uint8Array(event.data as ArrayBuffer),
				);
				if (message.msg.case === "gossip") {
					clearTimeout(timeout);
					resolve(message);
				}
			});
		},
	);
	const request = create(RelayClientMessageSchema, {
		msg: {
			case: "svExchange",
			value: {
				docId: `festival/${festivalId}/state`,
				sv: Y.encodeStateVector(new Y.Doc()),
			},
		},
	});
	ws.send(toBinary(RelayClientMessageSchema, request));
	const checkpoint = await response;
	ws.close();
	return checkpoint;
}

async function assertArtistProfileCheckpoint(festivalId: string, artistId: string) {
	const checkpoint = await signedCheckpoint(festivalId);
	expect(checkpoint.msg.case).toBe("gossip");
	if (checkpoint.msg.case !== "gossip") return;
	const update = checkpoint.msg.value.message?.payload;
	expect(update?.case).toBe("festivalUpdate");
	if (update?.case !== "festivalUpdate" || !update.value.signedUpdate) return;

	const doc = new Y.Doc();
	Y.applyUpdate(doc, update.value.signedUpdate.update);
	const artist = doc.getMap<Y.Map<unknown>>("artists").get(artistId);
	expect(artist?.get("description")).toBe("An offline artist profile.");
	expect(artist?.get("genres")).toBe(JSON.stringify(["electronic", "breakbeat"]));
	const set = [...doc.getMap<Y.Map<unknown>>("sets").values()][0];
	expect(set?.get("artistIds")).toBe(JSON.stringify([artistId]));
}

async function assertArtistProfileRoundTrip(festivalId: string, seededState: Uint8Array) {
	const localDoc = new Y.Doc();
	Y.applyUpdate(localDoc, seededState);
	const previousStateVector = Y.encodeStateVector(localDoc);
	const setsMap = localDoc.getMap<Y.Map<unknown>>("sets");
	const firstSet = [...setsMap.values()][0];
	expect(firstSet).toBeInstanceOf(Y.Map);
	if (!(firstSet instanceof Y.Map)) throw new Error("Expected imported set map");

	const artistId = "artist-00000000-0000-4000-8000-000000000001";
	const artistsMap = localDoc.getMap<Y.Map<unknown>>("artists");
	const artistMap = new Y.Map<unknown>();
	artistMap.set("name", "Artist One");
	artistMap.set("mbid", "00000000-0000-4000-8000-000000000001");
	artistMap.set("aliases", JSON.stringify(["Artist 1"]));
	artistMap.set("artistType", "Group");
	artistMap.set("country", "GB");
	artistMap.set("genres", JSON.stringify(["electronic", "breakbeat"]));
	artistMap.set("description", "An offline artist profile.");
	artistMap.set(
		"links",
		JSON.stringify([{ kind: "spotify", url: "https://open.spotify.com/artist/example" }]),
	);
	artistMap.set("provenance", JSON.stringify([]));
	artistMap.set("updatedAt", "2027-01-01T00:00:00.000Z");
	artistsMap.set(artistId, artistMap);
	firstSet.set("artistIds", JSON.stringify([artistId]));

	const admin = keypair();
	const addAdmin = await worker.fetch(`/festivals/${festivalId}/admins`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ publicKey: admin.publicKey }),
	});
	expect(addAdmin.status).toBe(200);

	const docId = `festival/${festivalId}/state`;
	const signResponse = await worker.fetch(`/festivals/${festivalId}/sign-update`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			publicKey: admin.publicKey,
			signature: bytesToHex(
				ed25519.sign(new TextEncoder().encode(`sign-update:${docId}`), admin.secretKey),
			),
			docId,
			topic: docId,
			update: bytesToBase64(Y.encodeStateAsUpdate(localDoc, previousStateVector)),
		}),
	});
	expect(signResponse.status).toBe(200);
	await assertArtistProfileCheckpoint(festivalId, artistId);
}

let worker: Unstable_DevWorker;

beforeAll(async () => {
	worker = await unstable_dev("src/index.ts", {
		persist: false,
		vars: {
			DEV_BYPASS_WEBAUTHN: "true",
			DISABLE_ARTIST_ENRICHMENT: "true",
			RP_ID: "localhost",
			MAIN_DO_ROOT_SECRET: "11".repeat(32),
			CLASHFINDER_TEST_FIXTURE: JSON.stringify(fixture),
		},
		experimental: { disableExperimentalWarning: true },
	});
});

afterAll(async () => {
	await worker.stop();
});

describe("registered-user Clashfinder imports", () => {
	it("requires registered-user authentication", async () => {
		const response = await worker.fetch("/festival-imports/preview", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ clashfinder: "community2027" }),
		});
		expect(response.status).toBe(401);
	});

	it("previews, publishes, seeds, deduplicates, and rejects replay", async () => {
		const identity = keypair();
		const { attestation } = await register(worker, identity.publicKey);
		const previewPath = "/festival-imports/preview";
		const previewBody = JSON.stringify({
			clashfinder: "https://clashfinder.com/s/community2027/",
		});
		const previewHeaders = authHeaders(
			previewPath,
			previewBody,
			identity,
			attestation,
		);
		const previewResponse = await worker.fetch(previewPath, {
			method: "POST",
			headers: previewHeaders,
			body: previewBody,
		});
		expect(previewResponse.status).toBe(200);
		const previewResult = (await previewResponse.json()) as {
			status: string;
			preview: { id: string; name: string; stageCount: number; setCount: number };
		};
		expect(previewResult).toMatchObject({
			status: "preview",
			preview: { name: fixture.name, stageCount: 1, setCount: 1 },
		});

		const replay = await worker.fetch(previewPath, {
			method: "POST",
			headers: previewHeaders,
			body: previewBody,
		});
		expect(replay.status).toBe(409);

		const publishPath = `/festival-imports/${previewResult.preview.id}/publish`;
		const publishBody = JSON.stringify({
			name: fixture.name,
			location: "Community Park",
			city: "Bristol",
			country: "GB",
		});
		// Pre-configure the FestivalDO without a lineup to prove publication
		// repairs partial initialization instead of skipping signed seeding.
		await worker.fetch("/festivals/cf-community2027/config", {
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				festivalId: "poisoned-id",
				opensAt: "2027-06-11T00:00:00.000Z",
				closesAt: "2027-06-14T23:59:59.999Z",
			}),
		});
		const publishResponse = await worker.fetch(publishPath, {
			method: "POST",
			headers: authHeaders(publishPath, publishBody, identity, attestation),
			body: publishBody,
		});
		expect(publishResponse.status).toBe(201);
		const published = (await publishResponse.json()) as {
			status: string;
			festival: { id: string; clashfinderId: string; city: string };
		};
		expect(published).toMatchObject({
			status: "created",
			festival: { id: "cf-community2027", clashfinderId: "community2027", city: "Bristol" },
		});

		const publishRetry = await worker.fetch(publishPath, {
			method: "POST",
			headers: authHeaders(publishPath, publishBody, identity, attestation),
			body: publishBody,
		});
		expect(publishRetry.status).toBe(200);
		expect(await publishRetry.json()).toMatchObject({
			status: "existing",
			festival: { id: published.festival.id },
		});

		const config = await worker.fetch(`/festivals/${published.festival.id}/config`);
		expect(config.status).toBe(200);
		expect(await config.json()).toMatchObject({ festivalId: published.festival.id });
		const festivals = (await (await worker.fetch("/festivals")).json()) as Array<{ id: string }>;
		expect(festivals.some((festival) => festival.id === published.festival.id)).toBe(true);

		const checkpoint = await signedCheckpoint(published.festival.id);
		let seededAuthoritySeq: bigint | undefined;
		let seededState: Uint8Array | undefined;
		expect(checkpoint.msg.case).toBe("gossip");
		if (checkpoint.msg.case === "gossip") {
			const update = checkpoint.msg.value.message?.payload;
			expect(update?.case).toBe("festivalUpdate");
			if (update?.case === "festivalUpdate") {
				expect(update.value.kind).toBe(FestivalUpdateKind.CHECKPOINT);
				seededAuthoritySeq = update.value.authoritySeq;
				const signed = update.value.signedUpdate;
				expect(signed).toBeDefined();
				const publicKey = await (
					await worker.fetch(`/festivals/${published.festival.id}/public-key`)
				).text();
				if (signed) {
					seededState = signed.update;
					expect(
						ed25519.verify(
							signed.signature,
							festivalUpdateSigningPayload(
								update.value.docId,
								update.value.kind,
								update.value.authoritySeq,
								signed.update,
							),
							hexToBytes(publicKey),
						),
					).toBe(true);
				}
			}
		}

		// Repeated public WS access re-runs ensureFestivalConfig, but an unchanged
		// canonical lineup must not allocate or broadcast another authority update.
		const repeatedCheckpoint = await signedCheckpoint(published.festival.id);
		expect(repeatedCheckpoint.msg.case).toBe("gossip");
		if (repeatedCheckpoint.msg.case === "gossip") {
			const repeatedUpdate = repeatedCheckpoint.msg.value.message?.payload;
			expect(repeatedUpdate?.case).toBe("festivalUpdate");
			if (repeatedUpdate?.case === "festivalUpdate") {
				expect(repeatedUpdate.value.authoritySeq).toBe(seededAuthoritySeq);
			}
		}

		// Artist profiles and their set references must survive the same signed
		// FestivalState checkpoint path consumed by offline clients.
		expect(seededState).toBeDefined();
		await assertArtistProfileRoundTrip(published.festival.id, seededState!);

		const duplicateBody = JSON.stringify({ clashfinder: "community2027" });
		const duplicate = await worker.fetch(previewPath, {
			method: "POST",
			headers: authHeaders(previewPath, duplicateBody, identity, attestation),
			body: duplicateBody,
		});
		expect(duplicate.status).toBe(200);
		expect(await duplicate.json()).toMatchObject({
			status: "existing",
			festival: { id: published.festival.id },
		});

		// A second festival may use the same common stage IDs.
		const secondPreviewBody = JSON.stringify({ clashfinder: "community-two-2027" });
		const secondPreviewResponse = await worker.fetch(previewPath, {
			method: "POST",
			headers: authHeaders(previewPath, secondPreviewBody, identity, attestation),
			body: secondPreviewBody,
		});
		const secondPreview = (await secondPreviewResponse.json()) as {
			preview: { id: string };
		};
		const secondPublishPath = `/festival-imports/${secondPreview.preview.id}/publish`;
		const secondPublishBody = JSON.stringify({
			name: "Community Festival Two",
			location: "Second Park",
			city: "Leeds",
			country: "GB",
		});
		const secondPublish = await worker.fetch(secondPublishPath, {
			method: "POST",
			headers: authHeaders(secondPublishPath, secondPublishBody, identity, attestation),
			body: secondPublishBody,
		});
		expect(secondPublish.status).toBe(201);
		expect(await secondPublish.json()).toMatchObject({
			festival: { id: "cf-community-two-2027" },
		});
	});

	it("bounds request size and per-user preview attempts", async () => {
		const oversized = await worker.fetch("/festival-imports/preview", {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ clashfinder: "x".repeat(70_000) }),
		});
		expect(oversized.status).toBe(413);

		const identity = keypair();
		const { attestation } = await register(worker, identity.publicKey);
		const path = "/festival-imports/preview";
		for (let index = 0; index < 10; index++) {
			const body = JSON.stringify({ clashfinder: `rate-limit-${index}` });
			const response = await worker.fetch(path, {
				method: "POST",
				headers: authHeaders(path, body, identity, attestation),
				body,
			});
			expect(response.status).toBe(200);
		}
		const limitedBody = JSON.stringify({ clashfinder: "rate-limit-final" });
		const limited = await worker.fetch(path, {
			method: "POST",
			headers: authHeaders(path, limitedBody, identity, attestation),
			body: limitedBody,
		});
		expect(limited.status).toBe(429);
		expect(limited.headers.get("Retry-After")).toBe("3600");
	});

	it("enforces preview ownership and publish limits", async () => {
		const owner = keypair();
		const ownerRegistration = await register(worker, owner.publicKey);
		const other = keypair();
		const otherRegistration = await register(worker, other.publicKey);
		const path = "/festival-imports/preview";
		const previewIds: string[] = [];
		for (let index = 0; index < 4; index++) {
			const body = JSON.stringify({ clashfinder: `publish-limit-${index}` });
			const response = await worker.fetch(path, {
				method: "POST",
				headers: authHeaders(path, body, owner, ownerRegistration.attestation),
				body,
			});
			const result = (await response.json()) as { preview: { id: string } };
			previewIds.push(result.preview.id);
		}

		const metadata = JSON.stringify({
			name: "Rate Limit Festival",
			location: "Test Park",
			city: "Cardiff",
			country: "GB",
		});
		const stolenPath = `/festival-imports/${previewIds[0]}/publish`;
		const stolen = await worker.fetch(stolenPath, {
			method: "POST",
			headers: authHeaders(stolenPath, metadata, other, otherRegistration.attestation),
			body: metadata,
		});
		expect(stolen.status).toBe(404);

		for (let index = 0; index < 3; index++) {
			const publishPath = `/festival-imports/${previewIds[index]}/publish`;
			const response = await worker.fetch(publishPath, {
				method: "POST",
				headers: authHeaders(publishPath, metadata, owner, ownerRegistration.attestation),
				body: metadata,
			});
			expect(response.status).toBe(201);
		}
		const limitedPath = `/festival-imports/${previewIds[3]}/publish`;
		const limited = await worker.fetch(limitedPath, {
			method: "POST",
			headers: authHeaders(limitedPath, metadata, owner, ownerRegistration.attestation),
			body: metadata,
		});
		expect(limited.status).toBe(429);

		const completedRetryPath = `/festival-imports/${previewIds[2]}/publish`;
		const completedRetry = await worker.fetch(completedRetryPath, {
			method: "POST",
			headers: authHeaders(
				completedRetryPath,
				metadata,
				owner,
				ownerRegistration.attestation,
			),
			body: metadata,
		});
		expect(completedRetry.status).toBe(200);
		expect(await completedRetry.json()).toMatchObject({ status: "existing" });
	});

	it("rejects stale and body-mismatched signatures", async () => {
		const identity = keypair();
		const { attestation } = await register(worker, identity.publicKey);
		const path = "/festival-imports/preview";
		const body = JSON.stringify({ clashfinder: "another2027" });
		const stale = await worker.fetch(path, {
			method: "POST",
			headers: authHeaders(path, body, identity, attestation, {
				timestamp: (Math.floor(Date.now() / 1000) - 1_000).toString(),
			}),
			body,
		});
		expect(stale.status).toBe(401);

		const mismatched = await worker.fetch(path, {
			method: "POST",
			headers: authHeaders(path, body, identity, attestation),
			body: JSON.stringify({ clashfinder: "tampered2027" }),
		});
		expect(mismatched.status).toBe(401);
	});
});
