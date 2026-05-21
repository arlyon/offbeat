import 'dart:convert';
import 'package:http/http.dart' as http;
import '../config.dart';

/// Orchestrates WebAuthn registration and attestation flows.
///
/// This service handles communication with the MainDO server for:
/// - New user registration (WebAuthn + attestation issuance)
/// - Device recovery (re-derive Ed25519 key from passkey)
/// - Attestation refresh (silent renewal)
class AuthService {
  final String _baseUrl;

  AuthService({String? baseUrl}) : _baseUrl = baseUrl ?? mainDoBaseUrl;

  /// Register a new identity.
  ///
  /// Flow:
  /// 1. POST /auth/register/begin → get WebAuthn challenge
  /// 2. Platform WebAuthn ceremony (passkeys package)
  /// 3. PRF derivation → Ed25519 keypair (via Rust bridge)
  /// 4. POST /auth/register/complete → get attestation
  ///
  /// Returns the attestation on success.
  Future<Map<String, dynamic>> register({
    required String ed25519PublicKeyHex,
  }) async {
    // Step 1: Begin registration
    final beginResp = await http.post(
      Uri.parse('$_baseUrl/auth/register/begin'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'userId': ed25519PublicKeyHex}),
    );
    if (beginResp.statusCode != 200) {
      throw AuthException('Registration begin failed: ${beginResp.body}');
    }
    // ignore: unused_local_variable
    final options = jsonDecode(beginResp.body);

    // Step 2: WebAuthn ceremony
    // TODO: Call passkeys package with options, get webauthnResponse
    // For now, pass empty response (auth stubs accept everything)
    final webauthnResponse = <String, dynamic>{};

    // Step 3: Complete registration with Ed25519 public key
    final completeResp = await http.post(
      Uri.parse('$_baseUrl/auth/register/complete'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'webauthnResponse': webauthnResponse,
        'ed25519PublicKey': ed25519PublicKeyHex,
      }),
    );
    if (completeResp.statusCode != 200) {
      throw AuthException('Registration complete failed: ${completeResp.body}');
    }

    final body = jsonDecode(completeResp.body) as Map<String, dynamic>;
    return body['attestation'] as Map<String, dynamic>;
  }

  /// Recover identity on a new device.
  ///
  /// Authenticates with existing passkey, verifies the Ed25519 key matches
  /// the one stored at registration.
  Future<Map<String, dynamic>> recover({
    required String ed25519PublicKeyHex,
  }) async {
    final beginResp = await http.post(
      Uri.parse('$_baseUrl/auth/recover/begin'),
      headers: {'Content-Type': 'application/json'},
    );
    if (beginResp.statusCode != 200) {
      throw AuthException('Recovery begin failed: ${beginResp.body}');
    }

    // TODO: WebAuthn assertion with PRF
    final assertion = <String, dynamic>{};

    final completeResp = await http.post(
      Uri.parse('$_baseUrl/auth/recover/complete'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'assertion': assertion,
        'ed25519PublicKey': ed25519PublicKeyHex,
      }),
    );
    if (completeResp.statusCode != 200) {
      throw AuthException('Recovery complete failed: ${completeResp.body}');
    }

    final body = jsonDecode(completeResp.body) as Map<String, dynamic>;
    return body['attestation'] as Map<String, dynamic>;
  }

  /// Silently refresh an expiring attestation.
  Future<Map<String, dynamic>> refresh({
    required String ed25519PublicKeyHex,
  }) async {
    // TODO: WebAuthn assertion for refresh
    final assertion = <String, dynamic>{};

    final resp = await http.post(
      Uri.parse('$_baseUrl/auth/refresh'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'assertion': assertion,
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

class AuthException implements Exception {
  final String message;
  AuthException(this.message);

  @override
  String toString() => 'AuthException: $message';
}
