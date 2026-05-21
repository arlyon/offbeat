import 'dart:convert';
import 'package:http/http.dart' as http;
import '../config.dart';

/// Handles admin operations against the MainDO and FestivalDO.
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
  Future<void> registerAdmin({
    required String publicKeyHex,
  }) async {
    final resp = await http.put(
      Uri.parse('$_baseUrl/admins'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'publicKey': publicKeyHex}),
    );
    if (resp.statusCode != 200) {
      throw AdminException('Admin registration failed: ${resp.body}');
    }
  }

  /// Register as a festival-specific admin on FestivalDO.
  /// First call is auto-accepted (bootstrap).
  Future<void> registerFestivalAdmin({
    required String festivalId,
    required String publicKeyHex,
  }) async {
    final resp = await http.put(
      Uri.parse('$_baseUrl/festivals/$festivalId/admins'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'publicKey': publicKeyHex}),
    );
    if (resp.statusCode != 200) {
      throw AdminException('Festival admin registration failed: ${resp.body}');
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

  /// Create a festival (admin-only).
  Future<Map<String, dynamic>> createFestival({
    required String publicKeyHex,
    required String pathSignatureHex,
    required Map<String, dynamic> festivalData,
  }) async {
    final resp = await http.post(
      Uri.parse('$_baseUrl/festivals'),
      headers: {
        'Content-Type': 'application/json',
        'X-Admin-Key': publicKeyHex,
        'X-Admin-Sig': pathSignatureHex,
      },
      body: jsonEncode(festivalData),
    );
    if (resp.statusCode != 201) {
      throw AdminException('Create festival failed: ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  /// Update festival metadata (admin-only).
  Future<Map<String, dynamic>> updateFestival({
    required String festivalId,
    required String publicKeyHex,
    required String pathSignatureHex,
    required Map<String, dynamic> updates,
  }) async {
    final resp = await http.put(
      Uri.parse('$_baseUrl/festivals/$festivalId'),
      headers: {
        'Content-Type': 'application/json',
        'X-Admin-Key': publicKeyHex,
        'X-Admin-Sig': pathSignatureHex,
      },
      body: jsonEncode(updates),
    );
    if (resp.statusCode != 200) {
      throw AdminException('Update festival failed: ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }

  /// Replace a festival's lineup (admin-only).
  Future<Map<String, dynamic>> replaceLineup({
    required String festivalId,
    required String publicKeyHex,
    required String pathSignatureHex,
    required Map<String, dynamic> lineupData,
  }) async {
    final resp = await http.put(
      Uri.parse('$_baseUrl/festivals/$festivalId/lineup'),
      headers: {
        'Content-Type': 'application/json',
        'X-Admin-Key': publicKeyHex,
        'X-Admin-Sig': pathSignatureHex,
      },
      body: jsonEncode(lineupData),
    );
    if (resp.statusCode != 200) {
      throw AdminException('Replace lineup failed: ${resp.body}');
    }
    return jsonDecode(resp.body) as Map<String, dynamic>;
  }
}

class AdminException implements Exception {
  final String message;
  AdminException(this.message);

  @override
  String toString() => 'AdminException: $message';
}
