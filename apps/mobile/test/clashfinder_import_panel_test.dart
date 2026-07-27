import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/models.dart';
import 'package:offbeat_mobile/screens/festival_list/clashfinder_import_panel.dart';
import 'package:offbeat_mobile/services/festival_import_service.dart';

Festival festival() => Festival.fromJson({
  'id': 'cf-test2027',
  'name': 'Test Festival',
  'year': 2027,
  'location': 'Test Park',
  'city': 'Bristol',
  'country': 'GB',
  'startDate': '2027-06-12',
  'endDate': '2027-06-13',
  'genres': <String>[],
  'stages': <Object>[],
});

Widget harness(Widget child) => MaterialApp(
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

void main() {
  test('request signing payload matches the server contract', () {
    expect(
      buildFestivalImportSigningPayload(
        path: '/festival-imports/preview',
        timestamp: '123',
        nonce: 'abc',
        body: '{"clashfinder":"event"}',
      ),
      'offbeat:festival-import:v1\nPOST\n/festival-imports/preview\n123\nabc\n{"clashfinder":"event"}',
    );
  });

  testWidgets('requires registration before previewing', (tester) async {
    var registered = false;
    await tester.pumpWidget(
      harness(
        ClashfinderImportPanel(
          registered: false,
          onRegister: () async => registered = true,
          onPreview: (_) async => throw UnimplementedError(),
          onPublish:
              ({
                required previewId,
                required name,
                required location,
                required city,
                required country,
              }) async => throw UnimplementedError(),
          onPublished: (_) async {},
          onClose: () {},
        ),
      ),
    );

    expect(
      find.text(
        'Register your device to add a public event. Registration prevents anonymous spam.',
      ),
      findsOneWidget,
    );
    await tester.tap(find.text('REGISTER'));
    await tester.pump();
    expect(registered, isTrue);
  });

  testWidgets('previews, validates metadata, and publishes', (tester) async {
    String? previewSource;
    ({String name, String location, String city, String country})?
    publishedMetadata;
    Festival? opened;
    final preview = ClashfinderPreview(
      id: 'preview-id',
      clashfinderId: 'test2027',
      name: 'Test Festival',
      startDate: DateTime(2027, 6, 12),
      endDate: DateTime(2027, 6, 13),
      stageCount: 4,
      setCount: 80,
      expiresAt: DateTime(2027, 1, 1),
    );

    await tester.pumpWidget(
      harness(
        ClashfinderImportPanel(
          registered: true,
          onRegister: () async {},
          onPreview: (source) async {
            previewSource = source;
            return ClashfinderPreviewResult.preview(preview);
          },
          onPublish:
              ({
                required previewId,
                required name,
                required location,
                required city,
                required country,
              }) async {
                expect(previewId, 'preview-id');
                publishedMetadata = (
                  name: name,
                  location: location,
                  city: city,
                  country: country,
                );
                return festival();
              },
          onPublished: (festival) async => opened = festival,
          onClose: () {},
        ),
      ),
    );

    final semantics = tester.ensureSemantics();
    expect(
      tester.getSemantics(find.byType(TextField)).label,
      contains('CLASHFINDER URL OR ID'),
    );
    await tester.enterText(
      find.byType(TextField),
      'https://clashfinder.com/s/test2027/',
    );
    await tester.tap(find.text('PREVIEW EVENT'));
    await tester.pumpAndSettle();
    expect(previewSource, 'https://clashfinder.com/s/test2027/');
    expect(
      find.text('2027-06-12 → 2027-06-13  //  4 STAGES  //  80 SETS'),
      findsOneWidget,
    );
    final confirmationFields = find.byType(TextField);
    for (final (index, label) in [
      'EVENT NAME',
      'VENUE',
      'CITY',
      'COUNTRY',
    ].indexed) {
      expect(
        tester.getSemantics(confirmationFields.at(index)).label,
        contains(label),
      );
    }

    await tester.tap(find.text('PUBLISH EVENT'));
    await tester.pump();
    expect(find.textContaining('two-letter country code'), findsOneWidget);
    expect(
      find.bySemanticsLabel(RegExp('Error:.*two-letter country code')),
      findsOneWidget,
    );

    final fields = find.byType(TextField);
    expect(fields, findsNWidgets(4));
    await tester.enterText(fields.at(1), 'Test Park');
    await tester.enterText(fields.at(2), 'Bristol');
    await tester.enterText(fields.at(3), 'gb');
    await tester.tap(find.text('PUBLISH EVENT'));
    await tester.pumpAndSettle();

    expect(publishedMetadata, (
      name: 'Test Festival',
      location: 'Test Park',
      city: 'Bristol',
      country: 'GB',
    ));
    expect(opened?.id, 'cf-test2027');
    semantics.dispose();
  });

  testWidgets('opens an already imported event from preview', (tester) async {
    Festival? opened;
    await tester.pumpWidget(
      harness(
        ClashfinderImportPanel(
          registered: true,
          onRegister: () async {},
          onPreview: (_) async => ClashfinderPreviewResult.existing(festival()),
          onPublish:
              ({
                required previewId,
                required name,
                required location,
                required city,
                required country,
              }) async => throw UnimplementedError(),
          onPublished: (festival) async => opened = festival,
          onClose: () {},
        ),
      ),
    );

    await tester.enterText(find.byType(TextField), 'test2027');
    await tester.tap(find.text('PREVIEW EVENT'));
    await tester.pumpAndSettle();
    expect(opened?.id, 'cf-test2027');
  });
}
