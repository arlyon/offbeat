import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import '../test/support/store_screenshot_app.dart';

const _scenes = [
  (StoreScreenshotScene.festivals, '01_festivals'),
  (StoreScreenshotScene.schedule, '02_schedule'),
  (StoreScreenshotScene.now, '03_now'),
  (StoreScreenshotScene.clashes, '04_clashes'),
];

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('capture deterministic store screenshots', (tester) async {
    if (Platform.isAndroid) {
      await binding.convertFlutterSurfaceToImage();
    }

    for (final (scene, name) in _scenes) {
      await tester.pumpWidget(StoreScreenshotApp(scene: scene));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 500));
      await binding.takeScreenshot(name);
    }
  });
}
