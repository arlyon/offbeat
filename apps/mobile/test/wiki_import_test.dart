import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import '../tool/import_psychonautwiki.dart' as importer;

void main() {
  test('fixture import preserves source-shaped route data and provenance', () {
    final fixture =
        jsonDecode(
              File('test/fixtures/psychonautwiki-mdma.json').readAsStringSync(),
            )
            as Map<String, dynamic>;
    final bundle =
        (fixture['bundles'] as List<dynamic>).single as Map<String, dynamic>;

    final first = importer.normalizePsychonautWikiBundle(bundle);
    final second = importer.normalizePsychonautWikiBundle(bundle);

    expect(jsonEncode(first), jsonEncode(second));
    expect(first['id'], 'mdma');
    expect(first['sourceRevision'], '123456');
    expect(first['retrievedAt'], '2026-08-11T00:00:00.000Z');
    expect(first['sourcePayloadSha256'], hasLength(64));
    expect(first['contentLicense'], contains('CC BY'));

    final route =
        (first['routes'] as List<dynamic>).single as Map<String, dynamic>;
    expect(route['name'], 'oral');
    expect((route['dose'] as Map<String, dynamic>)['units'], 'mg');
    expect(
      ((route['dose'] as Map<String, dynamic>)['common']
          as Map<String, dynamic>),
      {'min': 80, 'max': 120},
    );
    expect(
      ((route['duration'] as Map<String, dynamic>)['total']
          as Map<String, dynamic>)['units'],
      'hours',
    );
  });

  test('fixture import rejects a fuzzy non-exact substance match', () {
    final fixture =
        jsonDecode(
              File('test/fixtures/psychonautwiki-mdma.json').readAsStringSync(),
            )
            as Map<String, dynamic>;
    final original =
        (fixture['bundles'] as List<dynamic>).single as Map<String, dynamic>;
    final modified = jsonDecode(jsonEncode(original)) as Map<String, dynamic>;
    final response = modified['graphqlResponse'] as Map<String, dynamic>;
    final data = response['data'] as Map<String, dynamic>;
    final substance =
        (data['substances'] as List<dynamic>).single as Map<String, dynamic>;
    substance['name'] = 'Not MDMA';

    expect(
      () => importer.normalizePsychonautWikiBundle(modified),
      throwsFormatException,
    );
  });
}
