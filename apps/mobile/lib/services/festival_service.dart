import 'dart:convert';
import 'package:http/http.dart' as http;
import '../config.dart';
import '../data/mock_data.dart';

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
}
