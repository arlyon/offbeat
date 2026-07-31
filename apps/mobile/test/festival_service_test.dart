import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:offbeat_mobile/services/festival_service.dart';

class StreamingClient extends http.BaseClient {
  final Stream<List<int>> body;

  StreamingClient(this.body);

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async =>
      http.StreamedResponse(body, 200);
}

class DeferredClient extends http.BaseClient {
  final requests = <Completer<http.StreamedResponse>>[];

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) {
    final completer = Completer<http.StreamedResponse>();
    requests.add(completer);
    return completer.future;
  }

  void complete(int index, String payload) {
    requests[index].complete(
      http.StreamedResponse(Stream.value(utf8.encode(payload)), 200),
    );
  }
}

class MemoryFestivalRegistryStore implements FestivalRegistryStore {
  StoredFestivalRegistry? value;
  bool failWrites = false;

  @override
  Future<StoredFestivalRegistry?> load() async => value;

  @override
  Future<bool> replace({
    required String payloadJson,
    required String fetchedAt,
    required String requestToken,
  }) async {
    if (failWrites) throw StateError('cache unavailable');
    final current = value?.requestToken;
    if (current != null &&
        current.compareTo('90000000000000000000') < 0 &&
        current.compareTo(requestToken) >= 0) {
      return false;
    }
    value = StoredFestivalRegistry(
      payloadJson: payloadJson,
      fetchedAt: fetchedAt,
      requestToken: requestToken,
    );
    return true;
  }
}

Map<String, Object?> festivalJson(String id, String startDate) => {
  'id': id,
  'name': 'Festival $id',
  'year': 2027,
  'location': 'Test Park',
  'city': 'Bristol',
  'country': 'GB',
  'startDate': startDate,
  'endDate': '2027-06-13',
  'genres': ['electronic'],
  'status': 'upcoming',
  'clashfinderId': id,
  'publicKey': '',
  'updatedAt': '2027-01-01T00:00:00Z',
  'lat': 51.45,
  'lon': -2.58,
  'stages': [
    {
      'id': 'main',
      'name': 'Main Stage',
      'short': 'MAIN',
      'color': '#ff2d8f',
      'order': 0,
    },
  ],
};

void main() {
  test(
    'successful refresh survives a service restart through the store',
    () async {
      final store = MemoryFestivalRegistryStore();
      final payload = jsonEncode([festivalJson('fieldday', '2027-06-12')]);
      final service = FestivalService(
        baseUrl: 'https://example.test',
        client: MockClient((_) async => http.Response(payload, 200)),
      );

      final refreshed = await service.refreshFestivals(store: store);
      expect(refreshed.persisted, isTrue);
      expect(refreshed.festivals.single.id, 'fieldday');

      final restartedService = FestivalService(
        baseUrl: 'https://example.test',
        client: MockClient((_) async => http.Response('', 503)),
      );
      final cached = await restartedService.loadCachedFestivals(store);
      expect(cached?.festivals.single.id, 'fieldday');
      expect(cached?.fetchedAt.isUtc, isTrue);
    },
  );

  test(
    'later successful refresh authoritatively replaces removed entries',
    () async {
      final store = MemoryFestivalRegistryStore();
      var payload = jsonEncode([festivalJson('old', '2027-06-12')]);
      final service = FestivalService(
        baseUrl: 'https://example.test',
        client: MockClient((_) async => http.Response(payload, 200)),
      );

      await service.refreshFestivals(store: store);
      final replacement = festivalJson('new', '2027-07-12')
        ..['endDate'] = '2027-07-13';
      payload = jsonEncode([replacement]);
      await service.refreshFestivals(store: store);

      final cached = await service.loadCachedFestivals(store);
      expect(cached?.festivals.map((festival) => festival.id), ['new']);
    },
  );

  test('out-of-order responses cannot overwrite a newer request', () async {
    final store = MemoryFestivalRegistryStore();
    final client = DeferredClient();
    final service = FestivalService(
      baseUrl: 'https://example.test',
      client: client,
    );
    final olderPayload = jsonEncode([festivalJson('older', '2027-06-12')]);
    final newerPayload = jsonEncode([festivalJson('newer', '2027-06-12')]);

    final older = service.refreshFestivals(store: store);
    final newer = service.refreshFestivals(store: store);
    while (client.requests.length < 2) {
      await Future<void>.delayed(Duration.zero);
    }
    client.complete(1, newerPayload);
    expect((await newer).persisted, isTrue);
    client.complete(0, olderPayload);
    expect((await older).persisted, isFalse);

    final cached = await service.loadCachedFestivals(store);
    expect(cached?.festivals.single.id, 'newer');
  });

  test('persisted request tokens survive a backward wall clock', () async {
    final store = MemoryFestivalRegistryStore()
      ..value = StoredFestivalRegistry(
        payloadJson: jsonEncode([festivalJson('cached', '2027-06-12')]),
        fetchedAt: '2027-01-01T00:00:00Z',
        requestToken: '00008000000000000000',
      );
    final payload = jsonEncode([festivalJson('fresh', '2027-06-12')]);
    final service = FestivalService(
      baseUrl: 'https://example.test',
      client: MockClient((_) async => http.Response(payload, 200)),
    );

    expect((await service.refreshFestivals(store: store)).persisted, isTrue);
    expect(store.value?.requestToken, '00008000000000000001');
  });

  test(
    'near-terminal request tokens roll over without poisoning refresh',
    () async {
      final store = MemoryFestivalRegistryStore()
        ..value = StoredFestivalRegistry(
          payloadJson: jsonEncode([festivalJson('cached', '2027-06-12')]),
          fetchedAt: '2027-01-01T00:00:00Z',
          requestToken: '99999999999999999998',
        );
      var requestCount = 0;
      final payload = jsonEncode([festivalJson('fresh', '2027-06-12')]);
      final service = FestivalService(
        baseUrl: 'https://example.test',
        client: MockClient((_) async {
          requestCount++;
          return http.Response(payload, 200);
        }),
      );

      expect((await service.refreshFestivals(store: store)).persisted, isTrue);
      expect((await service.refreshFestivals(store: store)).persisted, isTrue);
      expect(requestCount, 2);
      expect(store.value?.requestToken, isNot('99999999999999999999'));
    },
  );

  test('failed refresh preserves the last successful cache', () async {
    final store = MemoryFestivalRegistryStore();
    final payload = jsonEncode([festivalJson('cached', '2027-06-12')]);
    var fail = false;
    final service = FestivalService(
      baseUrl: 'https://example.test',
      client: MockClient(
        (_) async => fail
            ? http.Response('unavailable', 503)
            : http.Response(payload, 200),
      ),
    );

    await service.refreshFestivals(store: store);
    fail = true;
    await expectLater(service.refreshFestivals(store: store), throwsException);
    final cached = await service.loadCachedFestivals(store);
    expect(cached?.festivals.single.id, 'cached');
  });

  test('oversized streamed responses are rejected incrementally', () async {
    final chunks = Stream<List<int>>.fromIterable([
      Uint8List(1024 * 1024),
      Uint8List(1024 * 1024),
      Uint8List(1),
    ]);
    final service = FestivalService(
      baseUrl: 'https://example.test',
      client: StreamingClient(chunks),
    );

    await expectLater(
      service.refreshFestivals(),
      throwsA(isA<FormatException>()),
    );
  });

  test('invalid calendar dates and stage orders are rejected', () async {
    final invalidDate = festivalJson('date', '2027-02-31')
      ..['endDate'] = '2027-03-01';
    final invalidOrder = festivalJson('order', '2027-06-12');
    final invalidStage =
        (invalidOrder['stages']! as List<Object?>).single
            as Map<String, Object?>;
    invalidStage['order'] = -1;
    final invalidYear = festivalJson('year', '2027-06-12')..['year'] = -1;
    for (final festival in [invalidDate, invalidOrder, invalidYear]) {
      final service = FestivalService(
        baseUrl: 'https://example.test',
        client: MockClient(
          (_) async => http.Response(jsonEncode([festival]), 200),
        ),
      );
      await expectLater(
        service.refreshFestivals(),
        throwsA(isA<FormatException>()),
      );
    }
  });

  test('extreme festival date ranges are rejected before expansion', () async {
    final invalid = festivalJson('extreme', '2000-01-01')
      ..['endDate'] = '2100-01-01';
    final service = FestivalService(
      baseUrl: 'https://example.test',
      client: MockClient(
        (_) async => http.Response(jsonEncode([invalid]), 200),
      ),
    );

    await expectLater(
      service.refreshFestivals(),
      throwsA(isA<FormatException>()),
    );
  });

  test(
    'corrupt cache is ignored and cache write failure keeps live data',
    () async {
      final store = MemoryFestivalRegistryStore()
        ..value = const StoredFestivalRegistry(
          payloadJson: '{not-json',
          fetchedAt: 'not-a-date',
          requestToken: '00000000000000000001',
        );
      final payload = jsonEncode([festivalJson('live', '2027-06-12')]);
      final service = FestivalService(
        baseUrl: 'https://example.test',
        client: MockClient((_) async => http.Response(payload, 200)),
      );

      expect(await service.loadCachedFestivals(store), isNull);
      store.failWrites = true;
      final refreshed = await service.refreshFestivals(store: store);
      expect(refreshed.festivals.single.id, 'live');
      expect(refreshed.persisted, isFalse);
    },
  );
}
