import 'dart:convert';
import 'package:http/http.dart' as http;
import '../config.dart';
import '../data/models.dart';

/// Fetches festival data from the MainDO server.
class FestivalService {
  final String _baseUrl;

  FestivalService({String? baseUrl}) : _baseUrl = baseUrl ?? mainDoBaseUrl;

  /// Fetch all festivals from the server.
  /// Returns a list of [Festival] objects parsed from the JSON response.
  Future<List<Festival>> fetchFestivals() async {
    final resp = await http.get(Uri.parse('$_baseUrl/festivals'));
    if (resp.statusCode != 200) {
      throw Exception('Failed to fetch festivals: ${resp.statusCode}');
    }
    final list = jsonDecode(resp.body) as List<dynamic>;
    return list.map((j) => Festival.fromJson(j as Map<String, dynamic>)).toList();
  }

  /// Fetch the Festival DO's Ed25519 public key (hex string).
  /// Returns null if the festival or key is not available.
  Future<String?> fetchFestivalPublicKey(String festivalId) async {
    try {
      final resp = await http.get(Uri.parse('$_baseUrl/festivals/$festivalId/public-key'));
      if (resp.statusCode == 200 && resp.body.length == 64) {
        return resp.body;
      }
    } catch (_) {}
    return null;
  }
}
