import { Hono } from "hono";
import type { ArtistEnrichmentMessage } from "./artist-enrichment";
import { MAX_IMPORT_REQUEST_BYTES } from "./festival-import";

type Env = {
	Bindings: {
		MAIN_DO: DurableObjectNamespace;
		FESTIVAL_DO: DurableObjectNamespace;
		ARTIST_ENRICHMENT_QUEUE: Queue<ArtistEnrichmentMessage>;
		ADMIN_SECRET_KEY?: string;
		MAIN_DO_ROOT_SECRET?: string;
		DEV_BYPASS_WEBAUTHN?: string;
		DISABLE_ARTIST_ENRICHMENT?: string;
		DISABLE_ARTIST_RESOLUTION?: string;
		AI_GATEWAY_BASE_URL?: string;
		AI_GATEWAY_TOKEN?: string;
		ARTIST_RESOLUTION_MODEL?: string;
		DEEPSEEK_API_KEY?: string;
		TAVILY_API_KEY?: string;
	};
};

const ANDROID_PACKAGE = "com.offbeat.offbeat_mobile";
const ANDROID_SHA256 =
	"B8:03:AB:79:63:E7:3B:91:6F:CE:BE:25:33:34:BC:87:BE:A3:08:4B:8C:CE:B8:A2:4E:80:A5:7D:F5:F3:AF:BA";

const app = new Hono<Env>();

// /.well-known/assetlinks.json — Android Digital Asset Links for passkey domain verification
app.get("/.well-known/assetlinks.json", (c) => {
	return c.json([
		{
			relation: [
				"delegate_permission/common.handle_all_urls",
				"delegate_permission/common.get_login_creds",
			],
			target: {
				namespace: "android_app",
				package_name: ANDROID_PACKAGE,
				sha256_cert_fingerprints: [ANDROID_SHA256],
			},
		},
	]);
});

function getMainDO(env: Env["Bindings"]) {
	const id = env.MAIN_DO.idFromName("main");
	return env.MAIN_DO.get(id);
}

interface MainArtistEnrichmentRpc {
	getArtistEnrichmentCandidates(
		festivalId: string,
		billingKeys?: string[],
	): Promise<ArtistEnrichmentMessage[]>;
	markArtistEnrichmentQueued(jobIds: string[]): Promise<void>;
}

interface ArtistEnqueueResult {
	queuedJobs: number;
	complete: boolean;
}

const MAX_ARTIST_QUEUE_BATCH_MESSAGES = 100;
const MAX_ARTIST_QUEUE_BATCH_BYTES = 240 * 1024;
const MAX_ARTIST_QUEUE_MESSAGE_BYTES = 128 * 1024;
const QUEUE_MESSAGE_OVERHEAD_BYTES = 64;

function artistEnrichmentBatches(
	candidates: readonly ArtistEnrichmentMessage[],
): ArtistEnrichmentMessage[][] {
	const encoder = new TextEncoder();
	const batches: ArtistEnrichmentMessage[][] = [];
	let batch: ArtistEnrichmentMessage[] = [];
	let batchBytes = 0;
	for (const candidate of candidates) {
		const messageBytes =
			encoder.encode(JSON.stringify(candidate)).byteLength + QUEUE_MESSAGE_OVERHEAD_BYTES;
		if (messageBytes > MAX_ARTIST_QUEUE_MESSAGE_BYTES) {
			throw new Error(`Artist enrichment message ${candidate.jobId} exceeds the queue size limit`);
		}
		if (
			batch.length > 0 &&
			(batch.length >= MAX_ARTIST_QUEUE_BATCH_MESSAGES ||
				batchBytes + messageBytes > MAX_ARTIST_QUEUE_BATCH_BYTES)
		) {
			batches.push(batch);
			batch = [];
			batchBytes = 0;
		}
		batch.push(candidate);
		batchBytes += messageBytes;
	}
	if (batch.length > 0) batches.push(batch);
	return batches;
}

export async function enqueueArtistEnrichment(
	env: Env["Bindings"],
	festivalId: string,
	billingKeys?: string[],
): Promise<ArtistEnqueueResult> {
	let candidates: ArtistEnrichmentMessage[];
	const main = getMainDO(env) as unknown as MainArtistEnrichmentRpc;
	try {
		candidates = await main.getArtistEnrichmentCandidates(festivalId, billingKeys);
	} catch (error) {
		console.error("[artist-enrichment] failed to load candidates", error);
		return { queuedJobs: 0, complete: false };
	}
	let batches: ArtistEnrichmentMessage[][];
	try {
		batches = artistEnrichmentBatches(candidates);
	} catch (error) {
		console.error("[artist-enrichment] failed to batch candidates", error);
		return { queuedJobs: 0, complete: false };
	}
	let queuedJobs = 0;
	for (const batch of batches) {
		try {
			await env.ARTIST_ENRICHMENT_QUEUE.sendBatch(batch.map((body) => ({ body })));
			queuedJobs += batch.length;
			await main.markArtistEnrichmentQueued(batch.map((candidate) => candidate.jobId));
		} catch (error) {
			console.error("[artist-enrichment] failed to send queue batch", error);
			return { queuedJobs, complete: false };
		}
	}
	return { queuedJobs, complete: true };
}

async function enqueueArtistResolutionFestivals(
	env: Env["Bindings"],
	festivalIds: readonly string[],
): Promise<{
	queuedFestivals: number;
	queuedJobs: number;
	failedFestivalIds: string[];
	disabled: boolean;
}> {
	if (env.DISABLE_ARTIST_ENRICHMENT === "true") {
		return {
			queuedFestivals: 0,
			queuedJobs: 0,
			failedFestivalIds: [...festivalIds],
			disabled: true,
		};
	}
	let queuedFestivals = 0;
	let queuedJobs = 0;
	const failedFestivalIds: string[] = [];
	for (const festivalId of festivalIds) {
		const result = await enqueueArtistEnrichment(env, festivalId);
		queuedJobs += result.queuedJobs;
		if (result.complete) {
			queuedFestivals += 1;
		} else {
			failedFestivalIds.push(festivalId);
		}
	}
	return { queuedFestivals, queuedJobs, failedFestivalIds, disabled: false };
}

function scheduleArtistEnrichment(
	c: { env: Env["Bindings"]; executionCtx: ExecutionContext },
	festivalId: string,
) {
	if (c.env.DISABLE_ARTIST_ENRICHMENT === "true") return;
	c.executionCtx.waitUntil(
		enqueueArtistEnrichment(c.env, festivalId).then((result) => {
			if (!result.complete) {
				console.error(`[artist-enrichment] failed to fully enqueue festival ${festivalId}`);
			}
		}),
	);
}

function requireUrl(value: string): URL {
	try {
		return new URL(value);
	} catch (error) {
		throw new Error(`Invalid URL: ${value}`, { cause: error });
	}
}

async function readRequestBodyWithinLimit(
	request: Request,
	maxBytes: number,
): Promise<ArrayBuffer | Response> {
	const contentLength = Number(request.headers.get("content-length"));
	if (Number.isFinite(contentLength) && contentLength > maxBytes) {
		return new Response("Import request is too large", { status: 413 });
	}
	if (!request.body) return new ArrayBuffer(0);
	const reader = request.body.getReader();
	const chunks: Uint8Array[] = [];
	let totalBytes = 0;
	try {
		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			totalBytes += value.byteLength;
			if (totalBytes > maxBytes) {
				await reader.cancel("request size limit exceeded");
				return new Response("Import request is too large", { status: 413 });
			}
			chunks.push(value);
		}
	} finally {
		reader.releaseLock();
	}
	const body = new Uint8Array(totalBytes);
	let offset = 0;
	for (const chunk of chunks) {
		body.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return body.buffer;
}

async function forwardToMainDOBounded(
	env: Env["Bindings"],
	path: string,
	request: Request,
): Promise<Response> {
	const body = await readRequestBodyWithinLimit(request, MAX_IMPORT_REQUEST_BYTES);
	if (body instanceof Response) return body;
	const stub = getMainDO(env);
	const url = requireUrl(request.url);
	url.pathname = path;
	return stub.fetch(
		new Request(url.toString(), {
			method: request.method,
			headers: request.headers,
			body,
		}),
	);
}

async function forwardToMainDO(
	env: Env["Bindings"],
	path: string,
	request: Request,
): Promise<Response> {
	const stub = getMainDO(env);
	const url = requireUrl(request.url);
	url.pathname = path;
	const body =
		request.method === "GET" || request.method === "HEAD" ? undefined : await request.arrayBuffer();
	return stub.fetch(
		new Request(url.toString(), {
			method: request.method,
			headers: request.headers,
			body,
		}),
	);
}

// GET /festivals
app.get("/festivals", (c) => {
	return forwardToMainDO(c.env, "/festivals", c.req.raw);
});

// POST /artist-resolutions/backfill — enqueue all existing festivals idempotently.
app.post("/artist-resolutions/backfill", async (c) => {
	const response = await forwardToMainDOBounded(c.env, "/artist-resolutions/backfill", c.req.raw);
	if (!response.ok) return response;
	const result = (await response.json()) as { festivalIds: string[] };
	const queued = await enqueueArtistResolutionFestivals(c.env, result.festivalIds);
	return Response.json(queued, { status: queued.failedFestivalIds.length > 0 ? 207 : 200 });
});

// GET /festivals/:id/lineup
app.get("/festivals/:id/lineup", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}/lineup`, c.req.raw);
});

// GET /festivals/:id/artist-resolutions — admin review list.
app.get("/festivals/:id/artist-resolutions", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}/artist-resolutions`, c.req.raw);
});

// GET /festivals/:id
app.get("/festivals/:id", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}`, c.req.raw);
});

// POST /festivals — create a new festival (admin-only, forwarded with auth headers)
app.post("/festivals", async (c) => {
	const response = await forwardToMainDO(c.env, "/festivals", c.req.raw);
	if (!response.ok) return response;
	const result = (await response.json()) as { festival?: { id: string } };
	if (result.festival?.id) {
		await ensureFestivalConfig(c.env, result.festival.id, true);
		scheduleArtistEnrichment(c, result.festival.id);
	}
	return Response.json(result, { status: response.status });
});

// POST /festival-imports/preview — registered-user Clashfinder preview.
app.post("/festival-imports/preview", async (c) => {
	const response = await forwardToMainDOBounded(c.env, "/festival-imports/preview", c.req.raw);
	if (!response.ok) return response;
	const result = (await response.json()) as {
		status: string;
		festival?: { id: string };
		preview?: unknown;
	};
	if (result.status === "existing" && result.festival?.id) {
		await ensureFestivalConfig(c.env, result.festival.id, true);
		scheduleArtistEnrichment(c, result.festival.id);
	}
	return Response.json(result, { status: response.status });
});

// POST /festival-imports/:previewId/publish — publish and seed authoritative state.
app.post("/festival-imports/:previewId/publish", async (c) => {
	const previewId = c.req.param("previewId");
	const path = `/festival-imports/${previewId}/publish`;
	const response = await forwardToMainDOBounded(c.env, path, c.req.raw);
	if (!response.ok) return response;
	const result = (await response.json()) as {
		status: string;
		festival?: { id: string };
	};
	if (result.festival?.id) {
		await ensureFestivalConfig(c.env, result.festival.id, true);
		scheduleArtistEnrichment(c, result.festival.id);
	}
	return Response.json(result, { status: response.status });
});

// GET /festivals/:id/artist-resolution-applications — signed-delivery status.
app.get("/festivals/:id/artist-resolution-applications", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}/artist-resolution-applications`, c.req.raw);
});

// POST /festivals/:id/artist-resolutions/retry — enqueue an idempotent backfill/retry.
app.post("/festivals/:id/artist-resolutions/retry", async (c) => {
	const id = c.req.param("id");
	const response = await forwardToMainDOBounded(
		c.env,
		`/festivals/${id}/artist-resolutions/retry`,
		c.req.raw,
	);
	if (!response.ok) return response;
	const result = (await response.json()) as { billingKeys?: string[] };
	if (c.env.DISABLE_ARTIST_ENRICHMENT === "true") {
		return Response.json({
			queuedFestivals: 0,
			queuedJobs: 0,
			failedFestivalIds: [id],
			disabled: true,
		});
	}
	const queued = await enqueueArtistEnrichment(c.env, id, result.billingKeys);
	return Response.json({
		queuedFestivals: queued.complete ? 1 : 0,
		queuedJobs: queued.queuedJobs,
		failedFestivalIds: queued.complete ? [] : [id],
		disabled: false,
	});
});

// PUT /artist-identities — create or update a provider-neutral canonical identity.
app.put("/artist-identities", (c) => {
	return forwardToMainDOBounded(c.env, "/artist-identities", c.req.raw);
});

// POST /artist-identities/search — search the canonical profile index.
app.post("/artist-identities/search", (c) => {
	return forwardToMainDOBounded(c.env, "/artist-identities/search", c.req.raw);
});

// POST /artist-identities/merge — merge one canonical identity into another.
app.post("/artist-identities/merge", (c) => {
	return forwardToMainDOBounded(c.env, "/artist-identities/merge", c.req.raw);
});

// PUT /festivals/:id/artist-resolutions — durable global manual override.
app.put("/festivals/:id/artist-resolutions", async (c) => {
	const id = c.req.param("id");
	const response = await forwardToMainDOBounded(
		c.env,
		`/festivals/${id}/artist-resolutions`,
		c.req.raw,
	);
	if (!response.ok) return response;
	const application = (await response.json()) as {
		resolution: { billingKey: string };
		profiles: unknown[];
		applications: Array<{ festivalId: string; setIds: string[] }>;
	};
	if (c.env.DISABLE_ARTIST_ENRICHMENT === "true") {
		return Response.json(
			{
				...application,
				queuedFestivals: 0,
				queuedJobs: 0,
				failedFestivalIds: application.applications.map((target) => target.festivalId),
				disabled: true,
			},
			{ status: 202 },
		);
	}
	let queuedFestivals = 0;
	let queuedJobs = 0;
	const failedFestivalIds: string[] = [];
	for (const target of application.applications) {
		const queued = await enqueueArtistEnrichment(c.env, target.festivalId, [
			application.resolution.billingKey,
		]);
		queuedJobs += queued.queuedJobs;
		if (queued.complete) queuedFestivals += 1;
		else failedFestivalIds.push(target.festivalId);
	}
	return Response.json(
		{
			...application,
			queuedFestivals,
			queuedJobs,
			failedFestivalIds,
			disabled: false,
		},
		{ status: failedFestivalIds.length > 0 ? 207 : 202 },
	);
});

// PUT /festivals/:id — update festival metadata (admin-only)
app.put("/festivals/:id", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}`, c.req.raw);
});

// DELETE /festivals/:id — delete a festival (admin-only)
app.delete("/festivals/:id", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}`, c.req.raw);
});

// PUT /festivals/:id/lineup — replace lineup (admin-only)
app.put("/festivals/:id/lineup", async (c) => {
	const id = c.req.param("id");
	const resp = await forwardToMainDO(c.env, `/festivals/${id}/lineup`, c.req.raw);
	if (!resp.ok) return resp;

	// Forward the updated lineup to the Festival DO if it's already configured
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const configResp = await stub.fetch(new Request("http://internal/config", { method: "GET" }));
	const config = (await configResp.json()) as { opensAt: string | null; closesAt: string | null };
	const lineup = await resp.json();
	if (config.opensAt && config.closesAt) {
		await (
			stub as unknown as {
				updateLineup(festivalId: string, lineup: unknown): Promise<void>;
			}
		).updateLineup(id, lineup);
	}
	scheduleArtistEnrichment(c, id);
	return Response.json(lineup, { status: resp.status });
});

// GET /auth/public-key — MainDO's attestation issuer key
app.get("/auth/public-key", (c) => {
	return forwardToMainDO(c.env, "/auth/public-key", c.req.raw);
});

// POST /auth/register/begin
app.post("/auth/register/begin", (c) => {
	return forwardToMainDO(c.env, "/auth/register/begin", c.req.raw);
});

// POST /auth/register/complete
app.post("/auth/register/complete", (c) => {
	return forwardToMainDO(c.env, "/auth/register/complete", c.req.raw);
});

// POST /auth/recover/begin — new device recovery
app.post("/auth/recover/begin", (c) => {
	return forwardToMainDO(c.env, "/auth/recover/begin", c.req.raw);
});

// POST /auth/recover/complete — verify assertion + Ed25519 key match
app.post("/auth/recover/complete", (c) => {
	return forwardToMainDO(c.env, "/auth/recover/complete", c.req.raw);
});

// POST /auth/refresh — re-issue attestation
app.post("/auth/refresh", (c) => {
	return forwardToMainDO(c.env, "/auth/refresh", c.req.raw);
});

// GET /festivals/:id/public-key — fetch Festival DO's Ed25519 public key
app.get("/festivals/:id/public-key", (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/public-key";
	return stub.fetch(new Request(url.toString(), { method: "GET" }));
});

// PUT /festivals/:id/config — set event window on Festival DO
app.put("/festivals/:id/config", async (c) => {
	if (c.env.DEV_BYPASS_WEBAUTHN !== "true") {
		return new Response("Not found", { status: 404 });
	}
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/config";
	return stub.fetch(
		new Request(url.toString(), {
			method: "PUT",
			body: await c.req.text(),
			headers: { "Content-Type": "application/json" },
		}),
	);
});

// GET /festivals/:id/config — read event window from Festival DO
app.get("/festivals/:id/config", (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/config";
	return stub.fetch(new Request(url.toString(), { method: "GET" }));
});

// PUT /admins — register a global admin on the MainDO
app.put("/admins", async (c) => {
	return forwardToMainDO(c.env, "/admins", c.req.raw);
});

// GET /admins — list global admins
app.get("/admins", (c) => {
	return forwardToMainDO(c.env, "/admins", c.req.raw);
});

// POST /admins/request — request to become an admin
app.post("/admins/request", (c) => {
	return forwardToMainDO(c.env, "/admins/request", c.req.raw);
});

// GET /admins/requests — list pending admin requests (admin-only)
app.get("/admins/requests", (c) => {
	return forwardToMainDO(c.env, "/admins/requests", c.req.raw);
});

// POST /admins/requests/:key/approve — approve an admin request
app.post("/admins/requests/:key/approve", (c) => {
	const key = c.req.param("key");
	return forwardToMainDO(c.env, `/admins/requests/${key}/approve`, c.req.raw);
});

// POST /admins/requests/:key/deny — deny an admin request
app.post("/admins/requests/:key/deny", (c) => {
	const key = c.req.param("key");
	return forwardToMainDO(c.env, `/admins/requests/${key}/deny`, c.req.raw);
});

// PUT /festivals/:id/admins — register an admin on a specific Festival DO
app.put("/festivals/:id/admins", async (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/admins";
	return stub.fetch(
		new Request(url.toString(), {
			method: "PUT",
			body: await c.req.text(),
			headers: {
				"Content-Type": "application/json",
				"X-Admin-Key": c.req.header("X-Admin-Key") ?? "",
			},
		}),
	);
});

// POST /festivals/:id/signing-key — export the Festival DO's signing key
app.post("/festivals/:id/signing-key", async (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/signing-key";
	return stub.fetch(
		new Request(url.toString(), {
			method: "POST",
			body: await c.req.text(),
			headers: { "Content-Type": "application/json" },
		}),
	);
});

// DELETE /festivals/:id/reset — wipe a Festival DO's storage (admin-only, forwarded via MainDO)
app.delete("/festivals/:id/reset", async (c) => {
	const id = c.req.param("id");
	// Admin auth is handled by forwarding to MainDO first
	const authResp = await forwardToMainDO(c.env, `/festivals/${id}/reset`, c.req.raw);
	if (!authResp.ok) return authResp;

	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/reset";
	return stub.fetch(new Request(url.toString(), { method: "DELETE" }));
});

// POST /festivals/:id/checkin — register a peer's endpoint in the CRDT
app.post("/festivals/:id/checkin", async (c) => {
	const id = c.req.param("id");

	// Auto-configure the DO's event window on first checkin
	await ensureFestivalConfig(c.env, id);

	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/checkin";
	return stub.fetch(
		new Request(url.toString(), {
			method: "POST",
			body: await c.req.text(),
			headers: {
				"Content-Type": "application/json",
				"X-Attestation-Message": c.req.header("X-Attestation-Message") ?? "",
				"X-Attestation-Signature": c.req.header("X-Attestation-Signature") ?? "",
				"X-Attestation-Issuer": c.req.header("X-Attestation-Issuer") ?? "",
				"X-Session-PublicKey": c.req.header("X-Session-PublicKey") ?? "",
				"X-Session-Signature": c.req.header("X-Session-Signature") ?? "",
				"X-Session-Timestamp": c.req.header("X-Session-Timestamp") ?? "",
			},
		}),
	);
});

// POST /festivals/:id/sign-update — sign + broadcast a Yrs update via the DO
app.post("/festivals/:id/sign-update", async (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = requireUrl(c.req.url);
	url.pathname = "/sign-update";
	return stub.fetch(
		new Request(url.toString(), {
			method: "POST",
			body: await c.req.text(),
			headers: { "Content-Type": "application/json" },
		}),
	);
});

/** Ensure the Festival DO has its event window configured and lineup seeded.
 *  Fetches metadata + lineup from MainDO and seeds the genesis Yrs doc. */
async function ensureFestivalConfig(
	env: Env["Bindings"],
	festivalId: string,
	requireLineup = false,
) {
	const doId = env.FESTIVAL_DO.idFromName(festivalId);
	const stub = env.FESTIVAL_DO.get(doId);
	const mainStub = getMainDO(env);
	const configUrl = requireUrl("http://internal/config");
	const existing = await stub.fetch(new Request(configUrl.toString(), { method: "GET" }));
	const config = (await existing.json()) as {
		opensAt: string | null;
		closesAt: string | null;
		festivalId: string | null;
	};

	if (!config.opensAt || !config.closesAt || config.festivalId !== festivalId) {
		const festUrl = requireUrl(`http://internal/festivals/${festivalId}`);
		const festResp = await mainStub.fetch(new Request(festUrl.toString()));
		if (!festResp.ok) return;
		const fest = (await festResp.json()) as {
			startDate: string;
			endDate: string;
			lat?: number;
			lon?: number;
		};
		const opens = new Date(fest.startDate);
		opens.setDate(opens.getDate() - 1);
		const closes = new Date(fest.endDate);
		closes.setDate(closes.getDate() + 1);
		closes.setHours(23, 59, 59, 999);
		await stub.fetch(
			new Request(configUrl.toString(), {
				method: "PUT",
				body: JSON.stringify({
					opensAt: opens.toISOString(),
					closesAt: closes.toISOString(),
					festivalId,
					lat: fest.lat,
					lon: fest.lon,
				}),
				headers: { "Content-Type": "application/json" },
			}),
		);
	}

	await syncAdminsToFestival(env, festivalId);
	// A lightweight completion marker keeps ordinary reconnects from transferring
	// and hashing the full lineup. Missing markers trigger convergent reconciliation.
	const festivalStub = stub as unknown as {
		hasSeededLineup(festivalId: string): Promise<boolean>;
		seedLineup(festivalId: string, lineup: unknown): Promise<void>;
	};
	if (!(await festivalStub.hasSeededLineup(festivalId))) {
		const lineupUrl = requireUrl(`http://internal/festivals/${festivalId}/lineup`);
		const lineupResp = await mainStub.fetch(new Request(lineupUrl.toString()));
		if (lineupResp.ok) {
			await festivalStub.seedLineup(festivalId, await lineupResp.json());
		} else if (requireLineup) {
			throw new Error(`Festival lineup unavailable for ${festivalId}`);
		}
	}
	await (stub as unknown as { armWeatherAlarm(): Promise<void> }).armWeatherAlarm();
}

/** Push global admin keys from MainDO into the Festival DO's admin table. */
async function syncAdminsToFestival(env: Env["Bindings"], festivalId: string) {
	const mainStub = getMainDO(env);
	const adminsResp = await mainStub.fetch(new Request("http://internal/admins", { method: "GET" }));
	if (!adminsResp.ok) return;

	const adminKeys = (await adminsResp.json()) as string[];
	if (adminKeys.length === 0) return;

	const doId = env.FESTIVAL_DO.idFromName(festivalId);
	const stub = env.FESTIVAL_DO.get(doId);
	// Use the DO's RPC method to import admins directly
	await (stub as unknown as { importAdmins(keys: string[]): Promise<void> }).importAdmins(
		adminKeys,
	);
}

// GET /festivals/:id/ws — WebSocket upgrade to Festival DO
app.get("/festivals/:id/ws", async (c) => {
	if (c.req.header("Upgrade")?.toLowerCase() !== "websocket") {
		return new Response("WebSocket upgrade required", {
			status: 426,
			headers: { Upgrade: "websocket" },
		});
	}
	const id = c.req.param("id");

	// Auto-configure the DO's event window on first WS connection
	await ensureFestivalConfig(c.env, id);
	// Always sync global admins (idempotent)
	await syncAdminsToFestival(c.env, id);

	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	return stub.fetch(c.req.raw);
});

export default app;
