import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

import '../config.dart';
import '../data/models.dart';
import '../src/rust/api.dart';

const _maxRegistryResponseBytes = 2 * 1024 * 1024;
const _registryTimeout = Duration(seconds: 10);
const _requestTokenRolloverFloor = '90000000000000000000';

class StoredFestivalRegistry {
  final String payloadJson;
  final String fetchedAt;
  final String requestToken;

  const StoredFestivalRegistry({
    required this.payloadJson,
    required this.fetchedAt,
    required this.requestToken,
  });
}

abstract interface class FestivalRegistryStore {
  Future<StoredFestivalRegistry?> load();

  Future<bool> replace({
    required String payloadJson,
    required String fetchedAt,
    required String requestToken,
  });
}

class LocalFestivalRegistryStore implements FestivalRegistryStore {
  final FestivalRegistryCacheStore store;

  const LocalFestivalRegistryStore(this.store);

  @override
  Future<StoredFestivalRegistry?> load() async {
    final cached = await store.load();
    if (cached == null) return null;
    return StoredFestivalRegistry(
      payloadJson: cached.payloadJson,
      fetchedAt: cached.fetchedAt,
      requestToken: cached.requestToken,
    );
  }

  @override
  Future<bool> replace({
    required String payloadJson,
    required String fetchedAt,
    required String requestToken,
  }) => store.replace(
    payloadJson: payloadJson,
    fetchedAt: fetchedAt,
    requestToken: requestToken,
  );
}

class AppNodeFestivalRegistryStore implements FestivalRegistryStore {
  final AppNode node;

  const AppNodeFestivalRegistryStore(this.node);

  @override
  Future<StoredFestivalRegistry?> load() async {
    final cached = await node.getFestivalRegistryCache();
    if (cached == null) return null;
    return StoredFestivalRegistry(
      payloadJson: cached.payloadJson,
      fetchedAt: cached.fetchedAt,
      requestToken: cached.requestToken,
    );
  }

  @override
  Future<bool> replace({
    required String payloadJson,
    required String fetchedAt,
    required String requestToken,
  }) => node.replaceFestivalRegistryCache(
    payloadJson: payloadJson,
    fetchedAt: fetchedAt,
    requestToken: requestToken,
  );
}

class FestivalRegistrySnapshot {
  final List<Festival> festivals;
  final DateTime fetchedAt;

  const FestivalRegistrySnapshot({
    required this.festivals,
    required this.fetchedAt,
  });
}

class FestivalRefreshResult extends FestivalRegistrySnapshot {
  final bool persisted;

  const FestivalRefreshResult({
    required super.festivals,
    required super.fetchedAt,
    required this.persisted,
  });
}

class FestivalService {
  final String _baseUrl;
  final http.Client _client;
  final bool _ownsClient;
  String _lastRequestToken = '00000000000000000000';
  Future<String> _requestTokenTail = Future.value('00000000000000000000');

  FestivalService({String? baseUrl, http.Client? client})
    : _baseUrl = baseUrl ?? mainDoBaseUrl,
      _client = client ?? http.Client(),
      _ownsClient = client == null;

  void dispose() {
    if (_ownsClient) _client.close();
  }

  Future<FestivalRegistrySnapshot?> loadCachedFestivals(
    FestivalRegistryStore store,
  ) async {
    final cached = await store.load();
    if (cached == null) return null;
    try {
      _observeRequestToken(cached.requestToken);
      return FestivalRegistrySnapshot(
        festivals: _parseFestivals(cached.payloadJson),
        fetchedAt: DateTime.parse(cached.fetchedAt).toUtc(),
      );
    } on FormatException {
      return null;
    } on TypeError {
      return null;
    }
  }

  /// Fetch the complete server-authoritative registry and replace its local cache.
  Future<FestivalRefreshResult> refreshFestivals({
    FestivalRegistryStore? store,
  }) async {
    final requestedAt = DateTime.now().toUtc();
    final requestToken = await _reserveRequestToken(store);
    final payloadJson = await _fetchRegistryPayload();
    final festivals = _parseFestivals(payloadJson);
    var persisted = false;
    if (store != null) {
      try {
        persisted = await store.replace(
          payloadJson: payloadJson,
          fetchedAt: requestedAt.toIso8601String(),
          requestToken: requestToken,
        );
      } catch (_) {
        // The fresh server result remains usable; the UI surfaces that offline
        // persistence could not be refreshed instead of discarding live data.
      }
    }
    return FestivalRefreshResult(
      festivals: festivals,
      fetchedAt: requestedAt,
      persisted: persisted,
    );
  }

  Future<String> _reserveRequestToken(FestivalRegistryStore? store) {
    final reservation = _requestTokenTail.then((_) async {
      if (store != null) {
        try {
          final cached = await store.load();
          if (cached != null) _observeRequestToken(cached.requestToken);
        } catch (_) {
          // A corrupt cache must not prevent a live server refresh.
        }
      }
      return _nextRequestToken();
    });
    _requestTokenTail = reservation.catchError((_) => _lastRequestToken);
    return reservation;
  }

  void _observeRequestToken(String token) {
    if (token != '99999999999999999999' &&
        token.compareTo(_requestTokenRolloverFloor) < 0 &&
        RegExp(r'^\d{20}$').hasMatch(token) &&
        token.compareTo(_lastRequestToken) > 0) {
      _lastRequestToken = token;
    }
  }

  String _nextRequestToken() {
    final wallToken = DateTime.now()
        .toUtc()
        .microsecondsSinceEpoch
        .toString()
        .padLeft(20, '0');
    _lastRequestToken =
        _lastRequestToken.compareTo(_requestTokenRolloverFloor) >= 0
        ? wallToken
        : wallToken.compareTo(_lastRequestToken) > 0
        ? wallToken
        : _incrementRequestToken(_lastRequestToken);
    return _lastRequestToken;
  }

  String _incrementRequestToken(String token) {
    final digits = token.codeUnits.toList();
    for (var index = digits.length - 1; index >= 0; index--) {
      if (digits[index] < 0x39) {
        digits[index]++;
        return String.fromCharCodes(digits);
      }
      digits[index] = 0x30;
    }
    throw StateError('Festival registry request token exhausted');
  }

  Future<String> _fetchRegistryPayload() async {
    final request = http.Request('GET', Uri.parse('$_baseUrl/festivals'));
    final response = await _client.send(request).timeout(_registryTimeout);
    if (response.statusCode != 200) {
      throw Exception('Failed to fetch festivals: ${response.statusCode}');
    }

    final bytes = BytesBuilder(copy: false);
    var byteCount = 0;
    await for (final chunk in response.stream.timeout(_registryTimeout)) {
      byteCount += chunk.length;
      if (byteCount > _maxRegistryResponseBytes) {
        throw const FormatException('Festival registry response is too large');
      }
      bytes.add(chunk);
    }
    return utf8.decode(bytes.takeBytes());
  }

  /// Fetch the Festival DO's Ed25519 public key (hex string).
  /// Returns null if the festival or key is not available.
  Future<String?> fetchFestivalPublicKey(String festivalId) async {
    try {
      final response = await _client
          .get(Uri.parse('$_baseUrl/festivals/$festivalId/public-key'))
          .timeout(_registryTimeout);
      if (response.statusCode == 200 && response.body.length == 64) {
        return response.body;
      }
    } catch (_) {}
    return null;
  }

  List<Festival> _parseFestivals(String payloadJson) {
    if (utf8.encode(payloadJson).length > _maxRegistryResponseBytes) {
      throw const FormatException('Festival registry response is too large');
    }
    final decoded = jsonDecode(payloadJson);
    if (decoded is! List<dynamic> || decoded.length > 2000) {
      throw const FormatException(
        'Festival registry must be a bounded JSON array',
      );
    }

    final festivalIds = <String>{};
    var totalStages = 0;
    return decoded
        .map((value) {
          if (value is! Map<String, dynamic>) {
            throw const FormatException('Festival registry entry is invalid');
          }
          final id = _requiredString(value, 'id');
          _requiredString(value, 'name');
          if (!festivalIds.add(id)) {
            throw FormatException('Duplicate festival ID: $id');
          }
          for (final field in [
            'location',
            'city',
            'country',
            'publicKey',
            'updatedAt',
          ]) {
            _boundedString(value, field);
          }
          if (value['clashfinderId'] != null) {
            _boundedString(value, 'clashfinderId');
          }
          final start = _festivalDate(value, 'startDate');
          final end = _festivalDate(value, 'endDate');
          if (end.isBefore(start) || end.difference(start).inDays > 31) {
            throw FormatException('Festival $id has an invalid date range');
          }
          if (start.year < 2000 || end.year > 2100) {
            throw FormatException(
              'Festival $id is outside the supported date range',
            );
          }
          final year = value['year'];
          if (year is! int || year < 2000 || year > 2100) {
            throw FormatException('Festival $id has an invalid year');
          }
          if (!const {'upcoming', 'live', 'past'}.contains(value['status'])) {
            throw FormatException('Festival $id has an invalid status');
          }
          final genres = value['genres'];
          if (genres is! List<dynamic> ||
              genres.length > 100 ||
              genres.any((genre) => genre is! String || genre.length > 1024)) {
            throw FormatException('Festival $id has invalid genres');
          }
          for (final coordinate in ['lat', 'lon']) {
            final number = value[coordinate];
            if (number != null && (number is! num || !number.isFinite)) {
              throw FormatException('Festival $id has invalid coordinates');
            }
          }

          final stages = value['stages'];
          if (stages is! List<dynamic> || stages.length > 500) {
            throw FormatException('Festival $id has invalid stages');
          }
          totalStages += stages.length;
          if (totalStages > 100000) {
            throw const FormatException(
              'Festival registry has too many stages',
            );
          }
          final stageIds = <String>{};
          for (final stage in stages) {
            if (stage is! Map<String, dynamic>) {
              throw FormatException('Festival $id has an invalid stage');
            }
            final stageId = _requiredString(stage, 'id');
            _requiredString(stage, 'name');
            _boundedString(stage, 'short');
            _boundedString(stage, 'color');
            final order = stage['order'];
            if (!stageIds.add(stageId) ||
                order is! int ||
                order < 0 ||
                order > 0xffffffff) {
              throw FormatException('Festival $id has an invalid stage');
            }
          }
          return Festival.fromJson(value);
        })
        .toList(growable: false);
  }

  String _requiredString(Map<String, dynamic> value, String field) {
    final text = value[field];
    if (text is! String || text.isEmpty || text.length > 1024) {
      throw FormatException('Festival registry has an invalid $field');
    }
    return text;
  }

  void _boundedString(Map<String, dynamic> value, String field) {
    final text = value[field];
    if (text is! String || text.length > 1024) {
      throw FormatException('Festival registry has an invalid $field');
    }
  }

  DateTime _festivalDate(Map<String, dynamic> value, String field) {
    final text = _requiredString(value, field);
    if (!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(text)) {
      throw FormatException('Festival registry has an invalid $field');
    }
    final date = DateTime.tryParse(text);
    final canonical = date == null
        ? null
        : '${date.year.toString().padLeft(4, '0')}-'
              '${date.month.toString().padLeft(2, '0')}-'
              '${date.day.toString().padLeft(2, '0')}';
    if (date == null || canonical != text) {
      throw FormatException('Festival registry has an invalid $field');
    }
    return date;
  }
}
