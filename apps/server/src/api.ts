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

// GET /festivals/:id/ws — WebSocket upgrade to Festival DO
app.get("/festivals/:id/ws", (c) => {
	const id = c.req.param("id");
	const doId = c.env.FESTIVAL_DO.idFromName(id);
	const stub = c.env.FESTIVAL_DO.get(doId);
	return stub.fetch(c.req.raw);
});

export default app;
