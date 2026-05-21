import { Hono } from "hono";

type Env = {
	Bindings: {
		MAIN_DO: DurableObjectNamespace;
		FESTIVAL_DO: DurableObjectNamespace;
	};
};

const app = new Hono<Env>();

function getMainDO(env: Env["Bindings"]) {
	const id = env.MAIN_DO.idFromName("main");
	return env.MAIN_DO.get(id);
}

function forwardToMainDO(env: Env["Bindings"], path: string, request: Request): Promise<Response> {
	const stub = getMainDO(env);
	const url = new URL(request.url);
	url.pathname = path;
	return stub.fetch(new Request(url.toString(), request));
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

// POST /auth/register/begin
app.post("/auth/register/begin", (c) => {
	return forwardToMainDO(c.env, "/auth/register/begin", c.req.raw);
});

// POST /auth/register/complete
app.post("/auth/register/complete", (c) => {
	return forwardToMainDO(c.env, "/auth/register/complete", c.req.raw);
});

// POST /auth/authenticate/begin
app.post("/auth/authenticate/begin", (c) => {
	return forwardToMainDO(c.env, "/auth/authenticate/begin", c.req.raw);
});

// POST /auth/authenticate/complete
app.post("/auth/authenticate/complete", (c) => {
	return forwardToMainDO(c.env, "/auth/authenticate/complete", c.req.raw);
});

// GET /festivals/:id/public-key — fetch Festival DO's Ed25519 public key
app.get("/festivals/:id/public-key", (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = new URL(c.req.url);
	url.pathname = "/public-key";
	return stub.fetch(new Request(url.toString(), { method: "GET" }));
});

// PUT /festivals/:id/config — set event window on Festival DO
app.put("/festivals/:id/config", async (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = new URL(c.req.url);
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
	const url = new URL(c.req.url);
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

// PUT /festivals/:id/admins — register an admin on a specific Festival DO
app.put("/festivals/:id/admins", async (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = new URL(c.req.url);
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
	const url = new URL(c.req.url);
	url.pathname = "/signing-key";
	return stub.fetch(
		new Request(url.toString(), {
			method: "POST",
			body: await c.req.text(),
			headers: { "Content-Type": "application/json" },
		}),
	);
});

// POST /festivals/:id/sign-update — sign + broadcast a Yrs update via the DO
app.post("/festivals/:id/sign-update", async (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	const url = new URL(c.req.url);
	url.pathname = "/sign-update";
	return stub.fetch(
		new Request(url.toString(), {
			method: "POST",
			body: await c.req.text(),
			headers: { "Content-Type": "application/json" },
		}),
	);
});

/** Ensure the Festival DO has its event window configured.
 *  Fetches start_date/end_date from MainDO and sets ±1 day. */
async function ensureFestivalConfig(env: Env["Bindings"], festivalId: string) {
	const doId = env.FESTIVAL_DO.idFromName(festivalId);
	const stub = env.FESTIVAL_DO.get(doId);

	// Check if already configured
	const configUrl = new URL("http://internal/config");
	const existing = await stub.fetch(new Request(configUrl.toString(), { method: "GET" }));
	const config = (await existing.json()) as { opensAt: string | null; closesAt: string | null };
	if (config.opensAt && config.closesAt) return;

	// Fetch festival metadata from MainDO
	const mainStub = getMainDO(env);
	const festUrl = new URL(`http://internal/festivals/${festivalId}`);
	const festResp = await mainStub.fetch(new Request(festUrl.toString()));
	if (!festResp.ok) return;

	const fest = (await festResp.json()) as { startDate: string; endDate: string };

	// ±1 day from start/end
	const opens = new Date(fest.startDate);
	opens.setDate(opens.getDate() - 1);
	const closes = new Date(fest.endDate);
	closes.setDate(closes.getDate() + 1);
	// Set to end of day
	closes.setHours(23, 59, 59, 999);

	await stub.fetch(
		new Request(configUrl.toString(), {
			method: "PUT",
			body: JSON.stringify({
				opensAt: opens.toISOString(),
				closesAt: closes.toISOString(),
			}),
			headers: { "Content-Type": "application/json" },
		}),
	);

	// Sync global admins to the Festival DO
	await syncAdminsToFestival(env, festivalId);
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
