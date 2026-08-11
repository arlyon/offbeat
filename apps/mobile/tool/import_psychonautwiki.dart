import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

const _apiUrl = 'https://api.psychonautwiki.org/';
const _mediaWikiApiUrl = 'https://psychonautwiki.org/w/api.php';
const _contentLicense = 'CC BY 4.0 semantic data';
const _contentLicenseUrl = 'https://psychonautwiki.org/wiki/Copyrights';
const _generatorVersion = 'offbeat-psychonautwiki-import/2';

typedef _GraphqlResult = ({
  Map<String, dynamic> response,
  String sourcePayloadSha256,
});

const _substances = <String, String>{
  '2c-b': '2C-B',
  '4-aco-dmt': '4-AcO-DMT',
  'alcohol': 'Alcohol',
  'alprazolam': 'Alprazolam',
  'amphetamine': 'Amphetamine',
  'caffeine': 'Caffeine',
  'cannabis': 'Cannabis',
  'cocaine': 'Cocaine',
  'codeine': 'Codeine',
  'diazepam': 'Diazepam',
  'dmt': 'DMT',
  'gabapentin': 'Gabapentin',
  'ghb': 'GHB',
  'heroin': 'Heroin',
  'ketamine': 'Ketamine',
  'lsd': 'LSD',
  'mdma': 'MDMA',
  'mephedrone': 'Mephedrone',
  'methamphetamine': 'Methamphetamine',
  'nitrous-oxide': 'Nitrous',
  'pregabalin': 'Pregabalin',
  'psilocybin-mushrooms': 'Psilocybin mushrooms',
  'tramadol': 'Tramadol',
};

const _graphqlQuery = r'''
query SubstanceForOfflineReference($query: String!) {
  substances(query: $query) {
    name
    url
    commonNames
    class {
      psychoactive
    }
    roas {
      name
      dose {
        units
        threshold
        light { min max }
        common { min max }
        strong { min max }
        heavy
      }
      duration {
        onset { min max units }
        duration { min max units }
        total { min max units }
      }
    }
  }
}
''';

Future<void> main(List<String> arguments) async {
  final fixturePath = _option(arguments, '--fixture');
  final outputPath =
      _option(arguments, '--output') ?? 'assets/wiki/generated/psychonautwiki';
  final onlyId = _option(arguments, '--only');
  final outputDirectory = Directory(outputPath)..createSync(recursive: true);

  try {
    final bundles = fixturePath == null
        ? await _fetchLive(onlyId: onlyId)
        : _readFixture(File(fixturePath), onlyId: onlyId);
    bundles.sort(
      (left, right) => (left['id'] as String).compareTo(right['id'] as String),
    );
    for (final bundle in bundles) {
      final normalized = _normalize(bundle);
      final id = normalized['id'] as String;
      final output = File('${outputDirectory.path}/$id.json');
      output.writeAsStringSync(
        '${const JsonEncoder.withIndent('  ').convert(normalized)}\n',
      );
      stdout.writeln('Wrote ${output.path}');
    }
  } on Object catch (error) {
    stderr.writeln('PsychonautWiki import failed: $error');
    exitCode = 1;
  }
}

Future<List<Map<String, dynamic>>> _fetchLive({String? onlyId}) async {
  final selected = _substances.entries.where(
    (entry) => onlyId == null || entry.key == onlyId,
  );
  if (selected.isEmpty) throw ArgumentError('Unknown substance id: $onlyId');

  final client = HttpClient()..userAgent = _generatorVersion;
  try {
    final bundles = <Map<String, dynamic>>[];
    for (final entry in selected) {
      final result = await _graphqlRequest(client, entry.value);
      final substance = _selectExactSubstance(result.response, entry.value);
      final sourceUrl = substance['url'] as String;
      final revision = await _fetchRevision(client, Uri.parse(sourceUrl));
      bundles.add({
        'id': entry.key,
        'queryName': entry.value,
        'retrievedAt': DateTime.now().toUtc().toIso8601String(),
        'graphqlResponse': result.response,
        'sourcePayloadSha256': result.sourcePayloadSha256,
        'revision': revision,
      });
    }
    return bundles;
  } finally {
    client.close(force: true);
  }
}

List<Map<String, dynamic>> _readFixture(File file, {String? onlyId}) {
  final bytes = file.readAsBytesSync();
  final decoded = jsonDecode(utf8.decode(bytes)) as Map<String, dynamic>;
  final sourcePayloadSha256 = sha256.convert(bytes).toString();
  final bundles = (decoded['bundles'] as List<dynamic>)
      .map(
        (value) => {
          ...value as Map<String, dynamic>,
          'sourcePayloadSha256': sourcePayloadSha256,
        },
      )
      .where((bundle) => onlyId == null || bundle['id'] == onlyId)
      .toList(growable: false);
  if (bundles.isEmpty) {
    throw ArgumentError('Fixture contains no matching records');
  }
  return bundles;
}

Future<_GraphqlResult> _graphqlRequest(
  HttpClient client,
  String queryName,
) async {
  final request = await client.postUrl(Uri.parse(_apiUrl));
  request.headers.contentType = ContentType.json;
  request.write(
    jsonEncode({
      'query': _graphqlQuery,
      'variables': {'query': queryName},
    }),
  );
  final response = await request.close();
  final bodyBytes = <int>[];
  await for (final chunk in response) {
    bodyBytes.addAll(chunk);
  }
  final body = utf8.decode(bodyBytes);
  if (response.statusCode != HttpStatus.ok) {
    throw HttpException('GraphQL returned ${response.statusCode}: $body');
  }
  final decoded = jsonDecode(body) as Map<String, dynamic>;
  if (decoded['errors'] != null) {
    throw FormatException('GraphQL errors: ${decoded['errors']}');
  }
  return (
    response: decoded,
    sourcePayloadSha256: sha256.convert(bodyBytes).toString(),
  );
}

Future<Map<String, dynamic>> _fetchRevision(
  HttpClient client,
  Uri sourceUrl,
) async {
  final title = Uri.decodeComponent(sourceUrl.pathSegments.last);
  final uri = Uri.parse(_mediaWikiApiUrl).replace(
    queryParameters: {
      'action': 'query',
      'prop': 'revisions',
      'rvprop': 'ids|timestamp',
      'titles': title,
      'format': 'json',
      'formatversion': '2',
    },
  );
  final request = await client.getUrl(uri);
  final response = await request.close();
  final body = await utf8.decoder.bind(response).join();
  if (response.statusCode != HttpStatus.ok) {
    throw HttpException('MediaWiki returned ${response.statusCode}: $body');
  }
  final decoded = jsonDecode(body) as Map<String, dynamic>;
  final query = decoded['query'] as Map<String, dynamic>;
  final pages = query['pages'] as List<dynamic>;
  final page = pages.single as Map<String, dynamic>;
  final revisions = page['revisions'] as List<dynamic>?;
  if (revisions == null || revisions.isEmpty) {
    throw FormatException('No source revision found for $title');
  }
  final revision = revisions.single as Map<String, dynamic>;
  return {
    'id': revision['revid'].toString(),
    'timestamp': revision['timestamp'] as String,
  };
}

Map<String, dynamic> normalizePsychonautWikiBundle(
  Map<String, dynamic> bundle,
) => _normalize(bundle);

Map<String, dynamic> _normalize(Map<String, dynamic> bundle) {
  final id = bundle['id'] as String;
  if (!_substances.containsKey(id)) {
    throw FormatException('Unknown fixture id: $id');
  }
  final queryName = bundle['queryName'] as String;
  final response = bundle['graphqlResponse'] as Map<String, dynamic>;
  final substance = _selectExactSubstance(response, queryName);
  final revision = bundle['revision'] as Map<String, dynamic>;
  final classes = substance['class'] as Map<String, dynamic>?;
  final routes = (substance['roas'] as List<dynamic>? ?? const [])
      .map((value) => value as Map<String, dynamic>)
      .where((route) => route['dose'] != null || route['duration'] != null)
      .map(_normalizeRoute)
      .toList(growable: false);

  return {
    'schemaVersion': 1,
    'id': id,
    'sourceName': substance['name'] as String,
    'sourceUrl': substance['url'] as String,
    'sourceRevision': revision['id']?.toString(),
    'sourceRevisionTimestamp': revision['timestamp'] as String?,
    'retrievedAt': bundle['retrievedAt'] as String,
    'contentLicense': _contentLicense,
    'contentLicenseUrl': _contentLicenseUrl,
    'graphqlQuery': _graphqlQuery.trim(),
    'sourcePayloadSha256':
        bundle['sourcePayloadSha256'] as String? ??
        sha256.convert(utf8.encode(jsonEncode(response))).toString(),
    'generatorVersion': _generatorVersion,
    'commonNames': List<String>.from(
      substance['commonNames'] as List<dynamic>? ?? const [],
    ),
    'psychoactiveClasses': List<String>.from(
      classes?['psychoactive'] as List<dynamic>? ?? const [],
    ),
    'routes': routes,
  };
}

Map<String, dynamic> _normalizeRoute(Map<String, dynamic> route) {
  final dose = route['dose'] as Map<String, dynamic>?;
  final duration = route['duration'] as Map<String, dynamic>?;
  return {
    'name': route['name'] as String,
    'dose': dose == null
        ? null
        : {
            'units': dose['units'],
            'threshold': dose['threshold'],
            'light': _normalizeRange(dose['light']),
            'common': _normalizeRange(dose['common']),
            'strong': _normalizeRange(dose['strong']),
            'heavy': dose['heavy'],
          },
    'duration': duration == null
        ? null
        : {
            'onset': _normalizeRange(duration['onset']),
            'duration': _normalizeRange(duration['duration']),
            'total': _normalizeRange(duration['total']),
          },
  };
}

Map<String, dynamic>? _normalizeRange(Object? value) {
  if (value == null) return null;
  final range = value as Map<String, dynamic>;
  return {
    'min': range['min'],
    'max': range['max'],
    if (range['units'] != null) 'units': range['units'],
  };
}

Map<String, dynamic> _selectExactSubstance(
  Map<String, dynamic> response,
  String queryName,
) {
  final data = response['data'] as Map<String, dynamic>?;
  final substances = data?['substances'] as List<dynamic>? ?? const [];
  final normalizedTarget = _normalizeName(queryName);
  for (final value in substances) {
    final substance = value as Map<String, dynamic>;
    if (_normalizeName(substance['name'] as String) == normalizedTarget) {
      return substance;
    }
  }
  throw FormatException('No exact result for $queryName');
}

String _normalizeName(String value) =>
    value.toLowerCase().replaceAll(RegExp(r'[^a-z0-9]+'), '').trim();

String? _option(List<String> arguments, String name) {
  final index = arguments.indexOf(name);
  if (index < 0) return null;
  if (index + 1 >= arguments.length) {
    throw ArgumentError('Missing value after $name');
  }
  return arguments[index + 1];
}
