import 'dart:convert';
import 'package:http/http.dart' as http;
import '../config.dart';

/// Handles global admin operations against the MainDO.
///
/// All admin endpoints require Ed25519 signatures. The signing is done
/// via the Rust bridge (AppNode.signMessage), and the public key + signature
/// are sent as headers or body fields.
class AdminService {
  final String _baseUrl;

  AdminService({String? baseUrl}) : _baseUrl = baseUrl ?? mainDoBaseUrl;

  /// Register as a global admin on MainDO.
  /// First call is auto-accepted (bootstrap). Subsequent calls require
  /// an existing admin to promote.
  Future<void> registerAdmin({required String publicKeyHex}) async {
    final resp = await http.put(
      Uri.parse('$_baseUrl/admins'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'publicKey': publicKeyHex}),
    );
    if (resp.statusCode != 200) {
      throw AdminException('Admin registration failed: ${resp.body}');
    }
  }

  /// Request to become an admin. Returns 'pending' or 'already_admin'.
  Future<String> requestAdmin({
    required String publicKeyHex,
    String? displayName,
  }) async {
    final resp = await http.post(
      Uri.parse('$_baseUrl/admins/request'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({
        'publicKey': publicKeyHex,
        if (displayName != null) 'displayName': displayName,
      }),
    );
    if (resp.statusCode != 200) {
      throw AdminException('Admin request failed: ${resp.body}');
    }
    final body = jsonDecode(resp.body) as Map<String, dynamic>;
    return body['status'] as String;
  }

  /// List pending admin requests.
  Future<List<AdminRequest>> listAdminRequests() async {
    final resp = await http.get(Uri.parse('$_baseUrl/admins/requests'));
    if (resp.statusCode != 200) {
      throw AdminException('List admin requests failed: ${resp.body}');
    }
    final list = jsonDecode(resp.body) as List<dynamic>;
    return list
        .map(
          (r) => AdminRequest(
            publicKey: r['publicKey'] as String,
            displayName: r['displayName'] as String,
            requestedAt: r['requestedAt'] as String,
          ),
        )
        .toList();
  }

  /// Approve a pending admin request (requires admin auth headers).
  Future<void> approveAdminRequest({
    required String publicKeyHex,
    required String adminKeyHex,
    required String adminSigHex,
  }) async {
    final resp = await http.post(
      Uri.parse('$_baseUrl/admins/requests/$publicKeyHex/approve'),
      headers: {
        'Content-Type': 'application/json',
        'X-Admin-Key': adminKeyHex,
        'X-Admin-Sig': adminSigHex,
      },
    );
    if (resp.statusCode != 200) {
      throw AdminException('Approve admin request failed: ${resp.body}');
    }
  }

  /// Deny a pending admin request (requires admin auth headers).
  Future<void> denyAdminRequest({
    required String publicKeyHex,
    required String adminKeyHex,
    required String adminSigHex,
  }) async {
    final resp = await http.post(
      Uri.parse('$_baseUrl/admins/requests/$publicKeyHex/deny'),
      headers: {
        'Content-Type': 'application/json',
        'X-Admin-Key': adminKeyHex,
        'X-Admin-Sig': adminSigHex,
      },
    );
    if (resp.statusCode != 200) {
      throw AdminException('Deny admin request failed: ${resp.body}');
    }
  }

  /// List all global admin public keys.
  Future<List<String>> listAdmins() async {
    final resp = await http.get(Uri.parse('$_baseUrl/admins'));
    if (resp.statusCode != 200) {
      throw AdminException('List admins failed: ${resp.body}');
    }
    final list = jsonDecode(resp.body) as List<dynamic>;
    return list.cast<String>();
  }
}

class AdminRequest {
  final String publicKey;
  final String displayName;
  final String requestedAt;

  AdminRequest({
    required this.publicKey,
    required this.displayName,
    required this.requestedAt,
  });
}

class AdminException implements Exception {
  final String message;
  AdminException(this.message);

  @override
  String toString() => 'AdminException: $message';
}
