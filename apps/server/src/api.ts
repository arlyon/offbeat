import { Hono } from "hono";
import { MAX_IMPORT_REQUEST_BYTES } from "./festival-import";

type Env = {
	Bindings: {
		MAIN_DO: DurableObjectNamespace;
		FESTIVAL_DO: DurableObjectNamespace;
		ADMIN_SECRET_KEY?: string;
		MAIN_DO_ROOT_SECRET?: string;
		DEV_BYPASS_WEBAUTHN?: string;
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

// GET /festivals/:id/lineup
app.get("/festivals/:id/lineup", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}/lineup`, c.req.raw);
});

// GET /festivals/:id
app.get("/festivals/:id", (c) => {
	const id = c.req.param("id");
	return forwardToMainDO(c.env, `/festivals/${id}`, c.req.raw);
});

// POST /festivals — create a new festival (admin-only, forwarded with auth headers)
app.post("/festivals", (c) => {
	return forwardToMainDO(c.env, "/festivals", c.req.raw);
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
	}
	return Response.json(result, { status: response.status });
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
	if (config.opensAt && config.closesAt) {
		const lineup = await resp.json();
		await (
			stub as unknown as {
				updateLineup(festivalId: string, lineup: unknown): Promise<void>;
			}
		).updateLineup(id, lineup);
		return Response.json(lineup);
	}

	return resp;
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
