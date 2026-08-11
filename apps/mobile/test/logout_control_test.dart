import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/serial_keyed_queue.dart';
import 'package:offbeat_mobile/screens/you/registration_screen.dart';
import 'package:offbeat_mobile/screens/you/you_screen.dart';
import 'package:offbeat_mobile/services/auth_service.dart';

Widget _app(Widget child) => MaterialApp(home: Scaffold(body: child));

YouScreen _youScreen({
  required Future<void> Function() onLogout,
  VoidCallback? onLogoutCompleted,
}) => YouScreen(
  userId: '0123456789abcdef',
  publicKeyHex: '01' * 32,
  displayName: 'Tester',
  authState: 'valid',
  isAdmin: false,
  adminKeys: const [],
  onDisplayNameChanged: (_) {},
  onLogout: onLogout,
  onLogoutCompleted: onLogoutCompleted,
);

Future<void> _openLogout(WidgetTester tester) async {
  await tester.scrollUntilVisible(
    find.text('LOG OUT'),
    300,
    scrollable: find.byType(Scrollable).first,
  );
  await tester.tap(find.text('LOG OUT'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'signed-out identity screen offers offline unlock and online setup',
    (tester) async {
      var unlocks = 0;
      var registrations = 0;
      await tester.pumpWidget(
        _app(
          RegistrationScreen(
            onUnlock: () async => unlocks++,
            onRegister: () async => registrations++,
          ),
        ),
      );

      expect(find.text('USE EXISTING PASSKEY'), findsOneWidget);
      expect(
        find.text('WORKS OFFLINE WHEN THE PASSKEY IS ON THIS DEVICE'),
        findsOneWidget,
      );
      expect(find.text('INTERNET REQUIRED'), findsOneWidget);

      await tester.tap(find.text('USE EXISTING PASSKEY'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('SET UP NEW PASSKEY'));
      await tester.pumpAndSettle();

      expect(unlocks, 1);
      expect(registrations, 1);
    },
  );

  testWidgets('passkey failures do not expose internal errors', (tester) async {
    await tester.pumpWidget(
      _app(
        RegistrationScreen(
          onUnlock: () async => throw StateError('sensitive unlock detail'),
          onRegister: () async => throw StateError('sensitive setup detail'),
        ),
      ),
    );

    await tester.tap(find.text('USE EXISTING PASSKEY'));
    await tester.pumpAndSettle();
    expect(
      find.text("COULDN'T UNLOCK THIS PASSKEY. TRY AGAIN."),
      findsOneWidget,
    );
    expect(find.textContaining('sensitive unlock detail'), findsNothing);

    await tester.tap(find.text('SET UP NEW PASSKEY'));
    await tester.pumpAndSettle();
    expect(
      find.text(
        "COULDN'T SET UP A PASSKEY. CHECK YOUR CONNECTION AND TRY AGAIN.",
      ),
      findsOneWidget,
    );
    expect(find.textContaining('sensitive setup detail'), findsNothing);
  });

  test(
    'offline assertion validation binds challenge and user verification',
    () {
      final challenge = base64Url
          .encode(List<int>.generate(32, (index) => index))
          .replaceAll('=', '');
      final clientData = base64Url
          .encode(
            utf8.encode(
              jsonEncode({'type': 'webauthn.get', 'challenge': challenge}),
            ),
          )
          .replaceAll('=', '');
      final authenticatorData = List<int>.filled(37, 0)..[32] = 0x05;
      final encodedAuthenticator = base64Url
          .encode(authenticatorData)
          .replaceAll('=', '');

      expect(
        () => validateOfflinePasskeyAssertion(
          clientDataJson: clientData,
          authenticatorData: encodedAuthenticator,
          signature: 'signed',
          expectedChallenge: challenge,
        ),
        returnsNormally,
      );
      expect(
        () => validateOfflinePasskeyAssertion(
          clientDataJson: clientData,
          authenticatorData: encodedAuthenticator,
          signature: 'signed',
          expectedChallenge: base64Url
              .encode(List<int>.filled(32, 9))
              .replaceAll('=', ''),
        ),
        throwsA(isA<AuthException>()),
      );
    },
  );

  test(
    'closing a mutation queue drains accepted work and rejects later work',
    () async {
      final queue = SerialKeyedQueue();
      final gate = Completer<void>();
      final actions = <String>[];
      final first = queue.enqueue('festival/set', () async {
        await gate.future;
        actions.add('first');
      });
      final second = queue.enqueue(
        'festival/set',
        () async => actions.add('second'),
      );
      final draining = queue.closeAndDrain();
      await queue.enqueue('festival/other', () async => actions.add('late'));

      gate.complete();
      await Future.wait([first, second, draining]);
      expect(actions, ['first', 'second']);
    },
  );

  testWidgets('logout requires confirmation and explains the data boundary', (
    tester,
  ) async {
    var logouts = 0;
    var completions = 0;
    await tester.pumpWidget(
      _app(
        _youScreen(
          onLogout: () async => logouts++,
          onLogoutCompleted: () => completions++,
        ),
      ),
    );

    await _openLogout(tester);
    expect(find.text('LOG OUT?'), findsOneWidget);
    expect(find.text('REMOVED FROM THIS DEVICE'), findsOneWidget);
    expect(find.text('KEPT OFFLINE'), findsOneWidget);
    expect(
      find.text('YOU CAN USE THIS PASSKEY TO LOG BACK IN WITHOUT INTERNET.'),
      findsOneWidget,
    );
    expect(logouts, 0);

    await tester.tap(find.text('CANCEL'));
    await tester.pumpAndSettle();
    expect(logouts, 0);

    await _openLogout(tester);
    await tester.tap(find.text('LOG OUT').last);
    await tester.pumpAndSettle();
    expect(logouts, 1);
    expect(completions, 1);
    expect(find.text('LOG OUT?'), findsNothing);
  });

  testWidgets('failed logout remains open with a safe error', (tester) async {
    await tester.pumpWidget(
      _app(
        _youScreen(
          onLogout: () async => throw StateError('sensitive internal error'),
        ),
      ),
    );

    await _openLogout(tester);
    await tester.tap(find.text('LOG OUT').last);
    await tester.pumpAndSettle();

    expect(find.text('LOG OUT?'), findsOneWidget);
    expect(
      find.text("COULDN'T LOG OUT. YOUR ACCOUNT IS STILL ON THIS DEVICE."),
      findsOneWidget,
    );
    expect(find.textContaining('sensitive internal error'), findsNothing);
  });
}
