import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

const _allowedCategories = {
  'emergency',
  'campsite',
  'mobility',
  'drug-testing',
  'substances',
  'meshtastic',
  'offbeat',
};
const _allowedPriorities = {'critical', 'high', 'normal'};
const _allowedStatuses = {
  'source-checked',
  'product-verified',
  'imported-unreviewed',
};
final _idPattern = RegExp(r'^[a-z0-9]+(?:[.-][a-z0-9]+)*$');
final _localePattern = RegExp(r'^[a-z]{2}(?:-[A-Z]{2})?$');
final _countryPattern = RegExp(r'^[A-Z]{2}$');
final _inlineHtmlPattern = RegExp(r'^\s*</?[a-z][^>]*>', multiLine: true);
final _internalLinkPattern = RegExp(r'\]\(wiki:([a-z0-9.-]+)\)');
final _markdownLinkPattern = RegExp(r'\]\(([^)]+)\)');

void main(List<String> arguments) {
  final checkOnly = arguments.contains('--check');
  final platform = _platformArgument(arguments);
  final root = Directory.current;
  final pagesDirectory = Directory('${root.path}/assets/wiki/pages');
  final generatedDirectory = Directory(
    '${root.path}/assets/wiki/generated/psychonautwiki',
  );
  final output = File(_outputPath(arguments, root));
  if (!pagesDirectory.existsSync()) {
    stderr.writeln('Missing ${pagesDirectory.path}');
    exitCode = 1;
    return;
  }

  try {
    final pages = pagesDirectory
        .listSync(recursive: true)
        .whereType<File>()
        .where((file) => file.path.endsWith('.md'))
        .map(_parsePage)
        .toList(growable: false);
    final generatedRecords =
        platform == _WikiPlatform.android && generatedDirectory.existsSync()
        ? generatedDirectory
              .listSync()
              .whereType<File>()
              .where((file) => file.path.endsWith('.json'))
              .map(_parseGeneratedRecord)
              .toList(growable: false)
        : <Map<String, dynamic>>[];
    generatedRecords.sort(
      (left, right) => (left['id'] as String).compareTo(right['id'] as String),
    );
    final platformPages = platform == _WikiPlatform.ios
        ? pages.map(_withoutGeneratedReferences).toList(growable: false)
        : pages;
    _validateCorpus(platformPages, generatedRecords);
    final supportedCountries =
        platformPages
            .expand((page) => page.metadata['countryCodes'] as List<dynamic>)
            .cast<String>()
            .toSet()
            .toList(growable: false)
          ..sort();
    final sortedPages = [...platformPages]
      ..sort((left, right) {
        final order = (left.metadata['order'] as int).compareTo(
          right.metadata['order'] as int,
        );
        if (order != 0) return order;
        return (left.metadata['id'] as String).compareTo(
          right.metadata['id'] as String,
        );
      });
    final corpus = <String, dynamic>{
      'schemaVersion': 1,
      'supportedCountries': supportedCountries,
      'generatedRecords': generatedRecords,
      'pages': [
        for (final page in sortedPages)
          {...page.metadata, 'markdown': page.markdown},
      ],
    };
    final corpusDigest = sha256
        .convert(utf8.encode(jsonEncode(corpus)))
        .toString();
    final generated = const JsonEncoder.withIndent(
      '  ',
    ).convert({...corpus, 'corpusDigest': corpusDigest});
    final contents = '$generated\n';

    if (checkOnly) {
      if (!output.existsSync() || output.readAsStringSync() != contents) {
        throw const FormatException(
          'assets/wiki/index.json is stale; run dart run tool/build_wiki.dart',
        );
      }
      stdout.writeln(
        'Wiki content is valid and the generated index is current.',
      );
      return;
    }

    output.parent.createSync(recursive: true);
    output.writeAsStringSync(contents);
    stdout.writeln(
      'Wrote ${output.path} with ${platformPages.length} pages for ${platform.name}.',
    );
  } on FormatException catch (error) {
    stderr.writeln('Wiki build failed: ${error.message}');
    exitCode = 1;
  }
}

enum _WikiPlatform { android, ios }

_WikiPlatform _platformArgument(List<String> arguments) {
  final index = arguments.indexOf('--platform');
  final value = index >= 0 && index + 1 < arguments.length
      ? arguments[index + 1]
      : arguments
                .cast<String?>()
                .firstWhere(
                  (argument) => argument?.startsWith('--platform=') ?? false,
                  orElse: () => null,
                )
                ?.substring('--platform='.length) ??
            'android';
  return switch (value) {
    'android' => _WikiPlatform.android,
    'ios' => _WikiPlatform.ios,
    _ => throw const FormatException('--platform must be android or ios'),
  };
}

String _outputPath(List<String> arguments, Directory root) {
  final index = arguments.indexOf('--output');
  if (index >= 0 && index + 1 < arguments.length) return arguments[index + 1];
  final inline = arguments.cast<String?>().firstWhere(
    (argument) => argument?.startsWith('--output=') ?? false,
    orElse: () => null,
  );
  return inline?.substring('--output='.length) ??
      '${root.path}/assets/wiki/index.json';
}

({Map<String, dynamic> metadata, String markdown, String path})
_withoutGeneratedReferences(
  ({Map<String, dynamic> metadata, String markdown, String path}) page,
) {
  if ((page.metadata['generatedRefs'] as List<dynamic>).isEmpty) return page;
  final metadata = {...page.metadata, 'generatedRefs': <String>[]};
  const heading = '\n## Imported reference data';
  final headingOffset = page.markdown.indexOf(heading);
  final markdown = headingOffset < 0
      ? page.markdown
      : '${page.markdown.substring(0, headingOffset).trimRight()}\n\n'
            '## Information limits\n\n'
            'OFFBEAT does not provide dose, route or duration reference data on iOS. '
            'Product identity, strength, contamination, interactions and individual response '
            'remain uncertain. Do not delay emergency care while trying to identify a substance.';
  return (metadata: metadata, markdown: markdown, path: page.path);
}

({Map<String, dynamic> metadata, String markdown, String path}) _parsePage(
  File file,
) {
  final raw = file.readAsStringSync().replaceAll('\r\n', '\n');
  if (!raw.startsWith('---\n')) {
    throw FormatException(
      '${file.path}: missing opening front-matter delimiter',
    );
  }
  final delimiter = raw.indexOf('\n---\n', 4);
  if (delimiter < 0) {
    throw FormatException(
      '${file.path}: missing closing front-matter delimiter',
    );
  }

  final metadataSource = raw.substring(4, delimiter);
  final markdown = raw.substring(delimiter + 5).trim();
  try {
    final metadata = jsonDecode(metadataSource) as Map<String, dynamic>;
    _validatePage(file.path, metadata, markdown);
    return (metadata: metadata, markdown: markdown, path: file.path);
  } on JsonUnsupportedObjectError catch (error) {
    throw FormatException('${file.path}: invalid metadata: $error');
  } on FormatException catch (error) {
    throw FormatException('${file.path}: ${error.message}');
  }
}

void _validatePage(
  String path,
  Map<String, dynamic> metadata,
  String markdown,
) {
  const requiredKeys = {
    'schemaVersion',
    'id',
    'locale',
    'title',
    'summary',
    'category',
    'countryCodes',
    'aliases',
    'tags',
    'generatedRefs',
    'priority',
    'order',
    'lastVerified',
    'contentStatus',
    'sources',
  };
  final unknownKeys = metadata.keys.toSet().difference(requiredKeys);
  final missingKeys = requiredKeys.difference(metadata.keys.toSet());
  if (unknownKeys.isNotEmpty || missingKeys.isNotEmpty) {
    throw FormatException(
      '$path: metadata keys differ; missing=$missingKeys unknown=$unknownKeys',
    );
  }
  if (metadata['schemaVersion'] != 1) {
    throw FormatException('$path: schemaVersion must be 1');
  }
  final id = metadata['id'];
  if (id is! String || !_idPattern.hasMatch(id)) {
    throw FormatException('$path: invalid id');
  }
  final locale = metadata['locale'];
  if (locale is! String || !_localePattern.hasMatch(locale)) {
    throw FormatException('$path: invalid locale');
  }
  for (final key in ['title', 'summary']) {
    if (metadata[key] is! String || (metadata[key] as String).trim().isEmpty) {
      throw FormatException('$path: $key must be non-empty');
    }
  }
  if (!_allowedCategories.contains(metadata['category'])) {
    throw FormatException('$path: invalid category');
  }
  if (!_allowedPriorities.contains(metadata['priority'])) {
    throw FormatException('$path: invalid priority');
  }
  if (!_allowedStatuses.contains(metadata['contentStatus'])) {
    throw FormatException('$path: invalid contentStatus');
  }
  if (metadata['order'] is! int || (metadata['order'] as int) < 0) {
    throw FormatException('$path: order must be a non-negative integer');
  }
  if (DateTime.tryParse(metadata['lastVerified'] as String? ?? '') == null) {
    throw FormatException('$path: invalid lastVerified date');
  }
  _validateStringList(path, metadata, 'countryCodes', _countryPattern);
  _validateStringList(path, metadata, 'aliases');
  _validateStringList(path, metadata, 'tags');
  _validateStringList(path, metadata, 'generatedRefs', _idPattern);

  final sources = metadata['sources'];
  if (sources is! List || sources.isEmpty) {
    throw FormatException('$path: sources must be a non-empty list');
  }
  for (final source in sources) {
    if (source is! Map<String, dynamic>) {
      throw FormatException('$path: source must be an object');
    }
    const allowed = {'title', 'publisher', 'url', 'revision', 'license'};
    if (source.keys.any((key) => !allowed.contains(key))) {
      throw FormatException('$path: source has unknown fields');
    }
    for (final key in ['title', 'publisher', 'url']) {
      if (source[key] is! String || (source[key] as String).trim().isEmpty) {
        throw FormatException('$path: source.$key must be non-empty');
      }
    }
    final uri = Uri.tryParse(source['url'] as String);
    if (uri == null || uri.scheme != 'https' || uri.host.isEmpty) {
      throw FormatException('$path: source URL must use https');
    }
  }
  if (markdown.isEmpty || !markdown.startsWith('# ')) {
    throw FormatException('$path: body must start with a level-one heading');
  }
  if (_inlineHtmlPattern.hasMatch(markdown)) {
    throw FormatException('$path: inline HTML is not allowed');
  }
  for (final match in _markdownLinkPattern.allMatches(markdown)) {
    final target = match.group(1)!;
    if (!target.startsWith('https://') && !target.startsWith('wiki:')) {
      throw FormatException('$path: unsupported link target $target');
    }
  }
}

void _validateStringList(
  String path,
  Map<String, dynamic> metadata,
  String key, [
  RegExp? pattern,
]) {
  final value = metadata[key];
  if (value is! List || value.any((item) => item is! String)) {
    throw FormatException('$path: $key must be a string list');
  }
  if (pattern != null &&
      value.cast<String>().any((item) => !pattern.hasMatch(item))) {
    throw FormatException('$path: $key contains an invalid value');
  }
}

Map<String, dynamic> _parseGeneratedRecord(File file) {
  final record = jsonDecode(file.readAsStringSync()) as Map<String, dynamic>;
  const requiredKeys = {
    'schemaVersion',
    'id',
    'sourceName',
    'sourceUrl',
    'sourceRevision',
    'sourceRevisionTimestamp',
    'retrievedAt',
    'contentLicense',
    'contentLicenseUrl',
    'graphqlQuery',
    'sourcePayloadSha256',
    'generatorVersion',
    'commonNames',
    'psychoactiveClasses',
    'routes',
  };
  final unknown = record.keys.toSet().difference(requiredKeys);
  final missing = requiredKeys.difference(record.keys.toSet());
  if (unknown.isNotEmpty || missing.isNotEmpty) {
    throw FormatException(
      '${file.path}: generated keys differ; missing=$missing unknown=$unknown',
    );
  }
  if (record['schemaVersion'] != 1 ||
      record['id'] is! String ||
      !_idPattern.hasMatch(record['id'] as String)) {
    throw FormatException('${file.path}: invalid generated record identity');
  }
  if (record['sourceName'] is! String ||
      record['sourceUrl'] is! String ||
      record['retrievedAt'] is! String ||
      record['contentLicense'] is! String ||
      record['sourcePayloadSha256'] is! String ||
      record['commonNames'] is! List ||
      record['psychoactiveClasses'] is! List ||
      record['routes'] is! List) {
    throw FormatException('${file.path}: invalid generated record fields');
  }
  final sourceUri = Uri.tryParse(record['sourceUrl'] as String);
  if (sourceUri == null || sourceUri.scheme != 'https') {
    throw FormatException('${file.path}: generated sourceUrl must use https');
  }
  if (!RegExp(
    r'^[a-f0-9]{64}$',
  ).hasMatch(record['sourcePayloadSha256'] as String)) {
    throw FormatException('${file.path}: invalid source payload hash');
  }
  return record;
}

void _validateCorpus(
  List<({Map<String, dynamic> metadata, String markdown, String path})> pages,
  List<Map<String, dynamic>> generatedRecords,
) {
  if (pages.isEmpty) throw const FormatException('No wiki pages found');
  final keys = <String>{};
  final ids = <String>{};
  final generatedIds = <String>{};
  for (final record in generatedRecords) {
    final id = record['id'] as String;
    if (!generatedIds.add(id)) {
      throw FormatException('Duplicate generated record $id');
    }
  }
  for (final page in pages) {
    final id = page.metadata['id'] as String;
    final locale = page.metadata['locale'] as String;
    if (!keys.add('$id@$locale')) {
      throw FormatException('Duplicate page $id@$locale');
    }
    ids.add(id);
  }
  for (final page in pages) {
    for (final reference in page.metadata['generatedRefs'] as List<dynamic>) {
      if (!generatedIds.contains(reference)) {
        throw FormatException(
          '${page.path}: missing generated record $reference',
        );
      }
    }
    for (final match in _internalLinkPattern.allMatches(page.markdown)) {
      final target = match.group(1)!;
      if (!ids.contains(target)) {
        throw FormatException(
          '${page.path}: broken internal link wiki:$target',
        );
      }
    }
  }
}
