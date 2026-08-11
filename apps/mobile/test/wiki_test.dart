import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/wiki/wiki_page.dart';
import 'package:offbeat_mobile/wiki/wiki_repository.dart';
import 'package:offbeat_mobile/wiki/wiki_screen.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('WikiRepository', () {
    test('loads the bundled GB pack and generated source records', () async {
      final catalog = await WikiRepository().load(countryCode: 'gb');

      expect(catalog.countryCode, 'GB');
      expect(catalog.countrySupported, isTrue);
      expect(catalog.pageById('emergency.get-help'), isNotNull);
      expect(
        catalog.generatedRecords.keys,
        containsAll(<String>[
          'cocaine',
          'ketamine',
          'mdma',
          '2c-b',
          'ghb',
          'cannabis',
          'alcohol',
          'lsd',
          'psilocybin-mushrooms',
          '4-aco-dmt',
        ]),
      );
      expect(
        catalog.generatedRecords['mdma']!.sourceUrl,
        startsWith('https://psychonautwiki.org/wiki/'),
      );
      expect(
        catalog.pages.expand((page) => page.generatedRefs).toSet(),
        catalog.generatedRecords.keys.toSet(),
      );
      expect(
        catalog.pages.map((page) => page.id),
        containsAll(<String>[
          'campsite.sun-heat-hydration',
          'campsite.nutrition-supplements-recovery',
          'mobility.hip-routine',
          'mobility.knee-routine',
          'mobility.ankle-calf-routine',
          'drug-testing.professional-and-at-home',
          'drug-testing.dangerous-combinations',
          'substances.cocaine',
          'substances.ketamine',
          'substances.mdma',
          'substances.2c-b',
          'substances.ghb-gbl',
          'substances.cannabis',
          'substances.alcohol',
          'substances.lsd',
          'substances.psilocybin-mushrooms',
          'substances.4-aco-dmt',
          'meshtastic.app-and-protocol',
          'meshtastic.offbeat-over-meshtastic',
          'offbeat.lineup',
          'offbeat.p2p-syncing',
          'offbeat.groups',
          'offbeat.likes-and-personal-schedule',
          'offbeat.bluetooth-sync',
          'offbeat.wifi-aware',
          'offbeat.weather',
        ]),
      );
    });

    test('does not leak GB guidance into an unsupported country', () async {
      final catalog = await WikiRepository().load(countryCode: 'US');

      expect(catalog.countrySupported, isFalse);
      expect(catalog.pages.every((page) => page.countryCodes.isEmpty), isTrue);
      expect(catalog.pageById('emergency.get-help'), isNull);
    });

    test('search ranks aliases and finds universal feature guides', () async {
      final repository = WikiRepository();
      final catalog = await repository.load(countryCode: 'GB');

      final urgentResults = repository.search(catalog.pages, 'overdose');
      final featureResults = repository.search(catalog.pages, 'meshtastic');

      expect(urgentResults, isNotEmpty);
      expect(urgentResults.first.id, 'emergency.get-help');
      expect(
        featureResults.map((page) => page.id),
        contains('meshtastic.app-and-protocol'),
      );
    });
  });

  testWidgets('opens urgent help from the offline guide home', (tester) async {
    final catalog = await tester.runAsync(
      () => WikiRepository().load(countryCode: 'GB'),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: WikiScreen(
          countryCode: 'GB',
          repository: _StaticWikiRepository(catalog!),
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(find.text('FIELD GUIDE'), findsOneWidget);
    expect(find.text('AVAILABLE OFFLINE'), findsOneWidget);
    final urgentAction = find.text('SOMEONE NEEDS HELP NOW');
    expect(urgentAction, findsOneWidget);
    expect(tester.getSize(urgentAction).height, greaterThan(0));

    await tester.tap(urgentAction);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));

    expect(find.text('Get emergency help'), findsWidgets);
    expect(find.text('SOURCES & ATTRIBUTION'), findsOneWidget);
    expect(find.textContaining('CALL 999 OR 112 NOW'), findsOneWidget);
  });

  testWidgets('renders imported dose data with a non-prescriptive warning', (
    tester,
  ) async {
    final catalog = await tester.runAsync(
      () => WikiRepository().load(countryCode: 'GB'),
    );
    final page = catalog!.pageById('substances.mdma')!;

    await tester.pumpWidget(
      MaterialApp(
        home: WikiArticleScreen(catalog: catalog, page: page),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(find.text('PSYCHONAUTWIKI REFERENCE DATA'), findsOneWidget);
    expect(find.textContaining('reproduced verbatim'), findsOneWidget);
    expect(find.textContaining('not safe doses'), findsOneWidget);
    expect(find.textContaining('SOURCE: PsychonautWiki'), findsOneWidget);
    expect(find.textContaining('LICENCE: CC BY 4.0'), findsOneWidget);
    expect(find.text('ORAL'), findsOneWidget);
    expect(find.text('Common'.toUpperCase()), findsOneWidget);
  });

  testWidgets('shows safe unsupported-country behavior', (tester) async {
    final catalog = await tester.runAsync(
      () => WikiRepository().load(countryCode: 'US'),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: WikiScreen(
          countryCode: 'US',
          repository: _StaticWikiRepository(catalog!),
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(
      find.textContaining('NO US COUNTRY PACK IS INSTALLED'),
      findsOneWidget,
    );
    expect(find.text('SOMEONE NEEDS HELP NOW'), findsNothing);
    expect(find.text('Get emergency help'), findsNothing);
  });
}

class _StaticWikiRepository extends WikiRepository {
  final WikiCatalog catalog;

  _StaticWikiRepository(this.catalog);

  @override
  Future<WikiCatalog> load({String? countryCode}) async => catalog;
}
