import { Hono } from "hono";

const app = new Hono();

app.get("/", (c) => c.text("OFFBEAT API"));

export default app;
