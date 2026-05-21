import * as jose from "jose";

const JWT_SECRET = new TextEncoder().encode("offbeat-dev-secret"); // TODO: env var

export async function generateRegistrationOptions(userId: string): Promise<object> {
	// Stub — will use @simplewebauthn/server
	return {
		challenge: crypto.getRandomValues(new Uint8Array(32)),
		rp: { name: "OFFBEAT", id: "localhost" },
		user: { id: userId, name: userId, displayName: userId },
	};
}

export async function verifyRegistration(response: unknown): Promise<{
	verified: boolean;
	credentialId?: string;
	publicKey?: string;
}> {
	// Stub — response param reserved for future @simplewebauthn/server integration
	void response;
	return { verified: true, credentialId: "stub", publicKey: "stub" };
}

export async function generateAuthenticationOptions(): Promise<object> {
	// Stub — will use @simplewebauthn/server
	return { challenge: crypto.getRandomValues(new Uint8Array(32)) };
}

export async function verifyAuthentication(response: unknown): Promise<{
	verified: boolean;
	userId?: string;
}> {
	// Stub — response param reserved for future @simplewebauthn/server integration
	void response;
	return { verified: true, userId: "stub" };
}

export async function createJwt(userId: string): Promise<string> {
	return new jose.SignJWT({ sub: userId })
		.setProtectedHeader({ alg: "HS256" })
		.setExpirationTime("24h")
		.setIssuedAt()
		.sign(JWT_SECRET);
}

export async function verifyJwt(token: string): Promise<{ userId: string }> {
	const { payload } = await jose.jwtVerify(token, JWT_SECRET);
	return { userId: payload.sub as string };
}
