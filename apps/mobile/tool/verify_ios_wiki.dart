import 'dart:convert';
import 'dart:io';

import 'build_wiki.dart' as build_wiki;

void main() {
  final temporaryDirectory = Directory.systemTemp.createTempSync(
    'offbeat-ios-wiki-verify.',
  );
  final output = File('${temporaryDirectory.path}/index.json');
  try {
    exitCode = 0;
    build_wiki.main(['--platform=ios', '--output=${output.path}']);
    if (exitCode != 0) {
      return;
    }

    if (!output.existsSync()) {
      stderr.writeln('iOS wiki verification did not produce ${output.path}.');
      exitCode = 1;
      return;
    }

    final encodedCorpus = output.readAsStringSync();
    final corpus = jsonDecode(encodedCorpus) as Map<String, dynamic>;
    if ((corpus['generatedRecords'] as List<dynamic>).isNotEmpty ||
        (corpus['pages'] as List<dynamic>).cast<Map<String, dynamic>>().any(
          (page) => (page['generatedRefs'] as List<dynamic>).isNotEmpty,
        ) ||
        encodedCorpus.contains('PSYCHONAUTWIKI REFERENCE DATA')) {
      stderr.writeln('iOS wiki corpus contains PsychonautWiki reference data.');
      exitCode = 1;
      return;
    }
    stdout.writeln('iOS wiki corpus excludes PsychonautWiki reference data.');
  } finally {
    temporaryDirectory.deleteSync(recursive: true);
  }
}
