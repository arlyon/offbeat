import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/models.dart';
import 'package:offbeat_mobile/screens/festival_detail/set_details_sheet.dart';

void main() {
  const stage = Stage(
    id: 'main',
    name: 'Main Stage',
    short: 'MAIN',
    color: 0xFFFF2D8F,
  );
  const day = Day(
    id: 'fri',
    label: 'Friday',
    dayNum: '12',
    month: 'June',
    year: 2027,
  );
  const profile = ArtistProfile(
    id: 'artist-mbid',
    name: 'Artist One',
    mbid: '00000000-0000-4000-8000-000000000000',
    country: 'GB',
    artistType: 'Group',
    genres: ['Electronic', 'Breakbeat'],
    description: 'A concise offline artist description.',
    links: [
      ArtistLink(
        kind: 'spotify',
        url: 'https://open.spotify.com/artist/example',
      ),
      ArtistLink(kind: 'soundcloud', url: 'http://soundcloud.com/insecure'),
      ArtistLink(kind: 'website', url: 'javascript:alert(1)'),
    ],
    updatedAt: '2027-01-01T00:00:00.000Z',
  );
  final set = FestSet(
    id: 'set-1',
    day: day.id,
    stage: stage.id,
    artist: profile.name,
    artistIds: const ['artist-mbid'],
    artistProfiles: const [profile],
    t: 720,
    dur: 60,
    genre: '',
  );

  testWidgets('renders an offline profile and only safe provider actions', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: TextButton(
              onPressed: () => showSetDetailsSheet(
                context,
                set: set,
                stages: const [stage],
                days: const [day],
                allSets: [set],
              ),
              child: const Text('OPEN'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('OPEN'));
    await tester.pumpAndSettle();

    expect(find.text('ABOUT'), findsOneWidget);
    expect(find.text('A'), findsOneWidget);
    expect(find.text(profile.description!), findsOneWidget);
    expect(find.text('ELECTRONIC'), findsOneWidget);
    expect(find.text('BREAKBEAT'), findsOneWidget);
    expect(find.text('SPOTIFY'), findsOneWidget);
    expect(find.text('SOUNDCLOUD'), findsNothing);
    expect(find.text('WEBSITE'), findsNothing);
  });
}
