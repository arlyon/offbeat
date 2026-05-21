import 'dart:convert';
import 'dart:typed_data';
import 'package:flutter_passkey_service/flutter_passkey_service.dart';
import 'package:flutter_passkey_service/pigeons/messages.g.dart';
import 'package:http/http.dart' as http;
import '../config.dart';

/// The PRF salt used to derive the Ed25519 identity key from a passkey.
/// Same salt = same key on every device and every authentication.
const _prfSalt = 'offbeat-ed25519-v1';

/// Base64url-encode a string for PRF input.
String _toBase64Url(String input) {
  return base64Url.encode(utf8.encode(input)).replaceAll('=', '');
}

/// Orchestrates WebAuthn registration and attestation flows.
///
/// This service handles communication with the MainDO server for:
/// - New user registration (WebAuthn + PRF key derivation + attestation)
/// - Device recovery (re-derive Ed25519 key from existing passkey)
/// - Attestation refresh (silent renewal)
class AuthService {
  final String _baseUrl;
  final String _rpId;

  AuthService({String? baseUrl, String? rpIdOverride})
    : _baseUrl = baseUrl ?? mainDoBaseUrl,
      _rpId = rpIdOverride ?? rpId;

  /// Register a new identity.
  ///
  /// 1. POST /auth/register/begin -> get WebAuthn challenge
  /// 2. Platform WebAuthn ceremony with PRF enabled
  /// 3. Authenticate immediately to get PRF output
  /// 4. PRF -> HKDF -> Ed25519 keypair (via Rust bridge)
  /// 5. POST /auth/register/complete -> get attestation
  ///
  /// [onPrfOutput] is called with the 32-byte PRF output for the Rust bridge
  /// to derive the Ed25519 key. It should return the hex public key.
  Future<RegistrationResult> register({
    required Future<String> Function(Uint8List prfOutput) onPrfOutput,
  }) async {
    // Step 1: Begin registration -- get challenge from server
    final beginResp = await http.post(
      Uri.parse('$_baseUrl/auth/register/begin'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'userId': 'new-user'}),
    );
    if (beginResp.statusCode != 200) {
      throw AuthException('Registration begin failed: ${beginResp.body}');
    }
    final serverOptions = jsonDecode(beginResp.body) as Map<String, dynamic>;
    final challenge = serverOptions['challenge'] as String;

    // Step 2: WebAuthn registration (PRF requested at auth time, not creation)
    final regOptions = FlutterPasskeyService.createRegistrationOptionsFromJson(
      serverOptions,
    );
    final CreatePasskeyResponseData regResponse;
    try {
      regResponse = await FlutterPasskeyService.register(regOptions);
    } on PasskeyException catch (e) {
      throw AuthException('Passkey registration: ${e.message} ${e.details}');
    }

    // Step 3: Authenticate immediately with PRF to derive key
    final prfSaltB64 = _toBase64Url(_prfSalt);
    final authOptions = FlutterPasskeyService.createAuthenticationOptions(
      challenge: challenge,
      rpId: _rpId,
      prfEval: {'first': prfSaltB64},
    );
    final GetPasskeyAuthenticationResponseData authResponse;
    try {
      authResponse = await FlutterPasskeyService.authenticate(authOptions);
    } on PasskeyException catch (e) {
      throw AuthException('Passkey auth: ${e.message} ${e.details}');
    }

    // Step 4: Extract PRF output and derive Ed25519 key via Rust bridge
    final prfResult =
        authResponse.clientExtensionResults?.prf?.results?['first'];
    if (prfResult == null) {
      throw AuthException(
        'PRF output not available -- platform may not support it',
      );
    }
    final prfBytes = base64Url.decode(base64Url.normalize(prfResult));
    final ed25519PublicKeyHex = await onPrfOutput(Uint8List.fromList(prfBytes));

    // Step 5: Complete registration with server — send full WebAuthn response
    final completeResp = await http.post(
      Uri.parse('$_baseUrl/auth/register/complete'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'webauthnResponse': {
          'id': regResponse.id,
          'rawId': regResponse.rawId,
          'type': regResponse.type,
          'response': {
            'clientDataJSON': regResponse.response.clientDataJSON,
            'attestationObject': regResponse.response.attestationObject,
            if (regResponse.response.transports != null)
              'transports': regResponse.response.transports,
          },
          if (regResponse.authenticatorAttachment != null)
            'authenticatorAttachment': regResponse.authenticatorAttachment,
        },
        'challenge': challenge,
        'ed25519PublicKey': ed25519PublicKeyHex,
      }),
    );
    if (completeResp.statusCode != 200) {
      throw AuthException('Registration complete failed: ${completeResp.body}');
    }

    final body = jsonDecode(completeResp.body) as Map<String, dynamic>;
    final attestation = body['attestation'] as Map<String, dynamic>;

    return RegistrationResult(
      ed25519PublicKeyHex: ed25519PublicKeyHex,
      attestation: attestation,
    );
  }

  /// Recover identity on a new device.
  ///
  /// Authenticates with existing passkey + PRF to re-derive the same Ed25519 key.
  /// Server confirms the key matches what was stored at registration.
  Future<RegistrationResult> recover({
    required Future<String> Function(Uint8List prfOutput) onPrfOutput,
  }) async {
    // Get challenge from server
    final beginResp = await http.post(
      Uri.parse('$_baseUrl/auth/recover/begin'),
      headers: {'Content-Type': 'application/json'},
    );
    if (beginResp.statusCode != 200) {
      throw AuthException('Recovery begin failed: ${beginResp.body}');
    }
    final serverOptions = jsonDecode(beginResp.body) as Map<String, dynamic>;
    final challenge = serverOptions['challenge'] as String;

    // Authenticate with PRF
    final prfSaltB64 = _toBase64Url(_prfSalt);
    final authOptions = FlutterPasskeyService.createAuthenticationOptions(
      challenge: challenge,
      rpId: _rpId,
      prfEval: {'first': prfSaltB64},
    );
    final GetPasskeyAuthenticationResponseData authResponse;
    try {
      authResponse = await FlutterPasskeyService.authenticate(authOptions);
    } on PasskeyException catch (e) {
      throw AuthException('Passkey auth: ${e.message} ${e.details}');
    }

    // Extract PRF output
    final prfResult =
        authResponse.clientExtensionResults?.prf?.results?['first'];
    if (prfResult == null) {
      throw AuthException('PRF output not available');
    }
    final prfBytes = base64Url.decode(base64Url.normalize(prfResult));
    final ed25519PublicKeyHex = await onPrfOutput(Uint8List.fromList(prfBytes));

    // Complete recovery -- send full assertion response
    final completeResp = await http.post(
      Uri.parse('$_baseUrl/auth/recover/complete'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'assertion': {
          'id': authResponse.id,
          'rawId': authResponse.rawId,
          'type': authResponse.type,
          'response': {
            'clientDataJSON': authResponse.response.clientDataJSON,
            'authenticatorData': authResponse.response.authenticatorData,
            'signature': authResponse.response.signature,
            if (authResponse.response.userHandle != null)
              'userHandle': authResponse.response.userHandle,
          },
          if (authResponse.authenticatorAttachment != null)
            'authenticatorAttachment': authResponse.authenticatorAttachment,
        },
        'challenge': challenge,
        'ed25519PublicKey': ed25519PublicKeyHex,
      }),
    );
    if (completeResp.statusCode != 200) {
      throw AuthException('Recovery complete failed: ${completeResp.body}');
    }

    final body = jsonDecode(completeResp.body) as Map<String, dynamic>;
    return RegistrationResult(
      ed25519PublicKeyHex: ed25519PublicKeyHex,
      attestation: body['attestation'] as Map<String, dynamic>,
    );
  }

  /// Silently refresh an expiring attestation.
  Future<Map<String, dynamic>> refresh({
    required String ed25519PublicKeyHex,
  }) async {
    // Get challenge
    final beginResp = await http.post(
      Uri.parse('$_baseUrl/auth/recover/begin'),
      headers: {'Content-Type': 'application/json'},
    );
    if (beginResp.statusCode != 200) {
      throw AuthException('Refresh begin failed: ${beginResp.body}');
    }
    final serverOptions = jsonDecode(beginResp.body) as Map<String, dynamic>;
    final challenge = serverOptions['challenge'] as String;

    // Silent WebAuthn assertion (no PRF needed for refresh)
    final authOptions = FlutterPasskeyService.createAuthenticationOptions(
      challenge: challenge,
      rpId: _rpId,
    );
    final GetPasskeyAuthenticationResponseData authResponse;
    try {
      authResponse = await FlutterPasskeyService.authenticate(authOptions);
    } on PasskeyException catch (e) {
      throw AuthException('Passkey auth: ${e.message} ${e.details}');
    }

    final resp = await http.post(
      Uri.parse('$_baseUrl/auth/refresh'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'assertion': {
          'id': authResponse.id,
          'rawId': authResponse.rawId,
          'type': authResponse.type,
          'response': {
            'clientDataJSON': authResponse.response.clientDataJSON,
            'authenticatorData': authResponse.response.authenticatorData,
            'signature': authResponse.response.signature,
            if (authResponse.response.userHandle != null)
              'userHandle': authResponse.response.userHandle,
          },
          if (authResponse.authenticatorAttachment != null)
            'authenticatorAttachment': authResponse.authenticatorAttachment,
        },
        'challenge': challenge,
        'ed25519PublicKey': ed25519PublicKeyHex,
      }),
    );
    if (resp.statusCode != 200) {
      throw AuthException('Refresh failed: ${resp.body}');
    }

    final body = jsonDecode(resp.body) as Map<String, dynamic>;
    return body['attestation'] as Map<String, dynamic>;
  }

  /// Fetch the MainDO's public key (for dev mode when not hardcoded).
  Future<String> fetchServerPublicKey() async {
    final resp = await http.get(Uri.parse('$_baseUrl/auth/public-key'));
    if (resp.statusCode != 200) {
      throw AuthException('Failed to fetch server public key: ${resp.body}');
    }
    return resp.body.trim();
  }
}

/// Result of a successful registration or recovery.
class RegistrationResult {
  final String ed25519PublicKeyHex;
  final Map<String, dynamic> attestation;

  RegistrationResult({
    required this.ed25519PublicKeyHex,
    required this.attestation,
  });
}

class AuthException implements Exception {
  final String message;
  AuthException(this.message);

  @override
  String toString() => 'AuthException: $message';
}
