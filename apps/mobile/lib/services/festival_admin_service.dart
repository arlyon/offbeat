import 'dart:convert';
import 'package:http/http.dart' as http;
import '../config.dart';
import 'admin_service.dart';

/// Handles festival-scoped admin operations against the FestivalDO.
class FestivalAdminService {
  final String _baseUrl;

  FestivalAdminService({String? baseUrl}) : _baseUrl = baseUrl ?? mainDoBaseUrl;

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
