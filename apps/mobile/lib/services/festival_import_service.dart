import 'dart:convert';
import 'dart:math';

import 'package:http/http.dart' as http;

import '../config.dart';
import '../data/models.dart';
import '../src/rust/api.dart';

String buildFestivalImportSigningPayload({
  required String path,
  required String timestamp,
  required String nonce,
  required String body,
}) => [
  'offbeat:festival-import:v1',
  'POST',
  path,
  timestamp,
  nonce,
  body,
].join('\n');

class ClashfinderPreview {
  final String id;
  final String clashfinderId;
  final String name;
  final DateTime startDate;
  final DateTime endDate;
  final int stageCount;
  final int setCount;
  final DateTime expiresAt;

  const ClashfinderPreview({
    required this.id,
    required this.clashfinderId,
    required this.name,
    required this.startDate,
    required this.endDate,
    required this.stageCount,
    required this.setCount,
    required this.expiresAt,
  });

  factory ClashfinderPreview.fromJson(Map<String, dynamic> json) {
    return ClashfinderPreview(
      id: json['id'] as String,
      clashfinderId: json['clashfinderId'] as String,
      name: json['name'] as String,
      startDate: DateTime.parse(json['startDate'] as String),
      endDate: DateTime.parse(json['endDate'] as String),
      stageCount: json['stageCount'] as int,
      setCount: json['setCount'] as int,
      expiresAt: DateTime.parse(json['expiresAt'] as String),
    );
  }
}

class ClashfinderPreviewResult {
  final ClashfinderPreview? preview;
  final Festival? existingFestival;

  const ClashfinderPreviewResult.preview(this.preview)
    : existingFestival = null;

  const ClashfinderPreviewResult.existing(this.existingFestival)
    : preview = null;
}

class FestivalImportService {
  final String _baseUrl;
  final http.Client _client;
  final Random _random;

  FestivalImportService({String? baseUrl, http.Client? client, Random? random})
    : _baseUrl = baseUrl ?? mainDoBaseUrl,
      _client = client ?? http.Client(),
      _random = random ?? Random.secure();

  Future<ClashfinderPreviewResult> preview({
    required AppNode node,
    required String clashfinder,
  }) async {
    const path = '/festival-imports/preview';
    final body = jsonEncode({'clashfinder': clashfinder});
    final response = await _client.post(
      Uri.parse('$_baseUrl$path'),
      headers: await _authHeaders(node, path, body),
      body: body,
    );
    final json = _decodeResponse(response);
    if (json['status'] == 'existing') {
      return ClashfinderPreviewResult.existing(
        Festival.fromJson(json['festival'] as Map<String, dynamic>),
      );
    }
    return ClashfinderPreviewResult.preview(
      ClashfinderPreview.fromJson(json['preview'] as Map<String, dynamic>),
    );
  }

  Future<Festival> publish({
    required AppNode node,
    required String previewId,
    required String name,
    required String location,
    required String city,
    required String country,
  }) async {
    final path = '/festival-imports/$previewId/publish';
    final body = jsonEncode({
      'name': name,
      'location': location,
      'city': city,
      'country': country,
    });
    final response = await _client.post(
      Uri.parse('$_baseUrl$path'),
      headers: await _authHeaders(node, path, body),
      body: body,
    );
    final json = _decodeResponse(response);
    return Festival.fromJson(json['festival'] as Map<String, dynamic>);
  }

  Future<Map<String, String>> _authHeaders(
    AppNode node,
    String path,
    String body,
  ) async {
    final attestation = await node.getAttestation();
    if (attestation == null) {
      throw const FestivalImportException('Register before adding an event.');
    }
    final publicKey = await node.getPublicKeyHex();
    final timestamp = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    final nonce = List<int>.generate(
      16,
      (_) => _random.nextInt(256),
    ).map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
    final payload = buildFestivalImportSigningPayload(
      path: path,
      timestamp: timestamp.toString(),
      nonce: nonce,
      body: body,
    );
    final signature = await node.signMessage(message: payload);
    return {
      'Content-Type': 'application/json',
      'X-Attestation-Message': attestation.message,
      'X-Attestation-Signature': attestation.signature,
      'X-Attestation-Issuer': attestation.issuer,
      'X-Session-PublicKey': publicKey,
      'X-Request-Timestamp': timestamp.toString(),
      'X-Request-Nonce': nonce,
      'X-Request-Signature': signature,
    };
  }

  Map<String, dynamic> _decodeResponse(http.Response response) {
    if (response.statusCode < 200 || response.statusCode >= 300) {
      final fallback = switch (response.statusCode) {
        401 || 403 => 'Registration expired. Register again.',
        404 => 'This preview expired. Preview the event again.',
        409 => 'This request was already used. Try again.',
        413 => 'That import is too large.',
        422 => 'Clashfinder could not provide a valid public event.',
        429 => 'Too many imports. Try again later.',
        _ => 'Could not import this event.',
      };
      final message = response.body.trim().isEmpty
          ? fallback
          : response.body.trim();
      throw FestivalImportException(message);
    }
    try {
      return jsonDecode(response.body) as Map<String, dynamic>;
    } catch (_) {
      throw const FestivalImportException(
        'The server returned an invalid import response.',
      );
    }
  }
}

class FestivalImportException implements Exception {
  final String message;
  const FestivalImportException(this.message);

  @override
  String toString() => message;
}
