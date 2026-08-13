import type {
	AuthenticationResponseJSON,
	RegistrationResponseJSON,
	WebAuthnCredential,
} from "@simplewebauthn/server";
import {
	generateAuthenticationOptions as genAuthOpts,
	generateRegistrationOptions as genRegOpts,
	verifyAuthenticationResponse,
	verifyRegistrationResponse,
} from "@simplewebauthn/server";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const _ANDROID_PACKAGE = "dev.arlyon.offbeat";

/** RP ID — the domain passkeys are bound to.
 *  In dev: "localhost". In production: "offbeat.app". */
/** RP ID — set via RP_ID in .dev.vars (local) or wrangler secret (deployed). */
export function getRpId(env: Record<string, unknown>): string {
	return (env.RP_ID as string) ?? "localhost";
}

/** Expected origin(s) for WebAuthn ceremonies.
 *  Includes the web origin and the Android app origin for native passkeys. */
export function getExpectedOrigins(env: Record<string, unknown>): string[] {
	const rpId = getRpId(env);
	const origins: string[] = [];

	// Android native passkey origin (Credential Manager)
	// The hash is the SHA256 cert fingerprint, base64url-encoded (from .dev.vars or wrangler secret)
	const apkHash = (env.ANDROID_APK_KEY_HASH as string) ?? "";
	if (apkHash) {
		origins.push(`android:apk-key-hash:${apkHash}`);
	}

	if (rpId === "localhost") {
		// Dev mode: accept any local address
		origins.push("http://localhost:8787");
		origins.push("http://localhost");
		// Also accept LAN IPs (common dev pattern)
		if (env.ALLOWED_ORIGINS) {
			origins.push(...(env.ALLOWED_ORIGINS as string).split(",").map((o) => o.trim()));
		}
	} else {
		origins.push(`https://${rpId}`);
	}

	return origins.filter((o) => o.length > 0);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

export async function generateRegistrationOptions(
	userId: string,
	env: Record<string, unknown>,
	existingCredentials: { id: string; transports?: string[] }[] = [],
): Promise<object> {
	const rpId = getRpId(env);
	const options = await genRegOpts({
		rpName: "OFFBEAT",
		rpID: rpId,
		userName: userId,
		attestationType: "none",
		authenticatorSelection: {
			residentKey: "preferred",
			userVerification: "preferred",
		},
		excludeCredentials: existingCredentials.map((c) => ({
			id: c.id,
			transports: c.transports as AuthenticatorTransport[] | undefined,
		})),
	});
	return options;
}

export async function verifyRegistration(
	response: unknown,
	expectedChallenge: string,
	env: Record<string, unknown>,
): Promise<{
	verified: boolean;
	credentialId?: string;
	publicKey?: Uint8Array;
	counter?: number;
	transports?: string[];
}> {
	const rpId = getRpId(env);
	const expectedOrigins = getExpectedOrigins(env);

	const verification = await verifyRegistrationResponse({
		response: response as RegistrationResponseJSON,
		expectedChallenge,
		expectedOrigin: expectedOrigins,
		expectedRPID: rpId,
		requireUserVerification: false,
	});

	if (!verification.verified || !verification.registrationInfo) {
		return { verified: false };
	}

	const { credential } = verification.registrationInfo;
	return {
		verified: true,
		credentialId: credential.id,
		publicKey: credential.publicKey,
		counter: credential.counter,
		transports: credential.transports,
	};
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

export async function generateAuthenticationOptions(
	env: Record<string, unknown>,
	allowCredentials: { id: string; transports?: string[] }[] = [],
): Promise<object> {
	const rpId = getRpId(env);
	const options = await genAuthOpts({
		rpID: rpId,
		userVerification: "preferred",
		allowCredentials: allowCredentials.map((c) => ({
			id: c.id,
			transports: c.transports as AuthenticatorTransport[] | undefined,
		})),
	});
	return options;
}

export async function verifyAuthentication(
	response: unknown,
	expectedChallenge: string,
	credential: WebAuthnCredential,
	env: Record<string, unknown>,
): Promise<{
	verified: boolean;
	newCounter?: number;
}> {
	const rpId = getRpId(env);
	const expectedOrigins = getExpectedOrigins(env);

	const verification = await verifyAuthenticationResponse({
		response: response as AuthenticationResponseJSON,
		expectedChallenge,
		expectedOrigin: expectedOrigins,
		expectedRPID: rpId,
		credential,
		requireUserVerification: false,
	});

	return {
		verified: verification.verified,
		newCounter: verification.authenticationInfo?.newCounter,
	};
}

// ---------------------------------------------------------------------------
// Helpers (kept for potential future use)
// ---------------------------------------------------------------------------

type AuthenticatorTransport =
	| "ble"
	| "cable"
	| "hybrid"
	| "internal"
	| "nfc"
	| "smart-card"
	| "usb";
