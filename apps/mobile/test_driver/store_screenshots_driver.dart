import 'dart:io';

import 'package:integration_test/integration_test_driver_extended.dart';

Future<void> main() async {
  final outputDirectory = Directory(
    Platform.environment['STORE_SCREENSHOT_OUTPUT'] ??
        'fastlane/generated-screenshots',
  )..createSync(recursive: true);
  final suffix = Platform.environment['STORE_SCREENSHOT_SUFFIX'] ?? '';

  await integrationDriver(
    onScreenshot:
        (
          String name,
          List<int> bytes, [
          Map<String, Object?>? arguments,
        ]) async {
          final filename = suffix.isEmpty ? '$name.png' : '${name}_$suffix.png';
          File('${outputDirectory.path}/$filename').writeAsBytesSync(bytes);
          stdout.writeln('Wrote ${outputDirectory.path}/$filename');
          return true;
        },
  );
}
