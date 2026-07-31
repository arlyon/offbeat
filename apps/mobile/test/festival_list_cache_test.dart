import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/models.dart';
import 'package:offbeat_mobile/screens/festival_list/festival_list_screen.dart';

Festival cachedFestival() => Festival.fromJson({
  'id': 'cached-festival',
  'name': 'Cached Festival',
  'year': 2027,
  'location': 'Test Park',
  'city': 'Bristol',
  'country': 'GB',
  'startDate': '2027-06-12',
  'endDate': '2027-06-13',
  'genres': <String>[],
  'stages': <Object>[],
});

FestivalListScreen screen({
  required List<Festival> festivals,
  required String error,
}) => FestivalListScreen(
  festivals: festivals,
  error: error,
  onFestivalTap: (_) {},
  onRefresh: () {},
  importRegistered: false,
  onRegister: () async {},
  onPreviewClashfinder: (_) async => throw UnimplementedError(),
  onPublishClashfinder:
      ({
        required previewId,
        required name,
        required location,
        required city,
        required country,
      }) async => throw UnimplementedError(),
  onFestivalPublished: (_) async {},
);

void main() {
  testWidgets('renders cached festivals with a visible stale timestamp', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: screen(
            festivals: [cachedFestival()],
            error: 'offline cache // updated 2027-06-01 12:30',
          ),
        ),
      ),
    );

    expect(find.text('Cached Festival'), findsOneWidget);
    expect(
      find.text('OFFLINE CACHE // UPDATED 2027-06-01 12:30'),
      findsOneWidget,
    );
  });

  testWidgets('shows an honest offline state when no cache exists', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: screen(festivals: const [], error: 'offline — no cached data'),
        ),
      ),
    );

    expect(find.text('OFFLINE — NO CACHED DATA'), findsOneWidget);
    expect(find.text('0 FESTIVALS'), findsOneWidget);
  });
}
