import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/group_schedule_overlay.dart';
import 'package:offbeat_mobile/data/models.dart';
import 'package:offbeat_mobile/data/serial_keyed_queue.dart';
import 'package:offbeat_mobile/screens/festival_detail/festival_detail_screen.dart';
import 'package:offbeat_mobile/screens/social/member_sheet.dart';
import 'package:offbeat_mobile/src/rust/api/dto.dart';
import 'package:offbeat_mobile/widgets/co_liker_pins.dart';

FestSet set({
  required String id,
  required String artist,
  required int start,
  bool starred = false,
  String day = 'fri',
  bool cancelled = false,
  bool likedByGroup = false,
  List<ScheduleSupporter> supporters = const [],
}) => FestSet(
  id: id,
  day: day,
  stage: 'main',
  artist: artist,
  t: start,
  dur: 60,
  genre: 'test',
  starred: starred,
  cancelled: cancelled,
  likedByGroup: likedByGroup,
  supporters: supporters,
);

void main() {
  test('schedule clashes use strict overlap and liked sets', () {
    final result = withScheduleClashes([
      set(id: 'liked', artist: 'Liked', start: 60, starred: true),
      set(id: 'overlap', artist: 'Overlap', start: 90),
      set(id: 'touching', artist: 'Touching', start: 120),
      set(id: 'other-day', artist: 'Other day', start: 90, day: 'sat'),
      set(
        id: 'cancelled',
        artist: 'Cancelled',
        start: 90,
        starred: true,
        cancelled: true,
      ),
    ]);
    final byId = {for (final value in result) value.id: value};

    expect(byId['liked']!.clashes, isEmpty);
    expect(byId['overlap']!.clashes, ['liked']);
    expect(byId['touching']!.clashes, isEmpty);
    expect(byId['other-day']!.clashes, isEmpty);
    expect(byId['cancelled']!.clashes, isEmpty);
  });

  test('two liked overlapping sets clash symmetrically', () {
    final result = withScheduleClashes([
      set(id: 'a', artist: 'A', start: 60, starred: true),
      set(id: 'b', artist: 'B', start: 90, starred: true),
    ]);
    expect(result[0].clashes, ['b']);
    expect(result[1].clashes, ['a']);
  });

  test('co-liker summary shows two first names and overflow', () {
    expect(
      compactSupporterSummary(const [
        ScheduleSupporter(userId: 'ali', displayName: 'Ali Jones'),
        ScheduleSupporter(userId: 'luke', displayName: 'Luke Smith'),
        ScheduleSupporter(userId: 'ornella', displayName: 'Ornella Diaz'),
      ]),
      'Ali · Luke · +1',
    );
  });

  test('serial keyed queue preserves rapid toggle order', () async {
    final queue = SerialKeyedQueue();
    final firstGate = Completer<void>();
    final events = <String>[];

    final first = queue.enqueue('festival/set', () async {
      events.add('first-start');
      await firstGate.future;
      events.add('first-end');
    });
    final second = queue.enqueue('festival/set', () async {
      events.add('second');
    });
    final unrelated = queue.enqueue('festival/other', () async {
      events.add('other');
    });

    await unrelated;
    expect(events, ['first-start', 'other']);
    firstGate.complete();
    await Future.wait([first, second]);
    expect(events, ['first-start', 'other', 'first-end', 'second']);
  });

  testWidgets('My Schedule shows only liked sets', (tester) async {
    final sets = withScheduleClashes([
      set(id: 'liked', artist: 'LIKED ARTIST', start: 60, starred: true),
      set(id: 'other', artist: 'OTHER ARTIST', start: 180),
    ]);
    final festival = Festival.fromJson({
      'id': 'fest',
      'name': 'Test Fest',
      'year': 2026,
      'location': 'Field',
      'city': 'City',
      'country': 'GB',
      'genres': <String>[],
      'stages': <Object>[],
    });

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FestivalDetailScreen(
            festival: festival,
            now: DateTime(2026),
            stages: const [
              Stage(id: 'main', name: 'Main', short: 'M', color: 0xFFFF2D8F),
            ],
            days: const [
              Day(
                id: 'fri',
                label: 'Friday',
                dayNum: '1',
                month: 'JUL',
                year: 2026,
              ),
            ],
            sets: sets,
          ),
        ),
      ),
    );

    await tester.tap(find.text('MINE'));
    await tester.tap(find.text('DAYS'));
    await tester.pumpAndSettle();

    expect(find.text('LIKED ARTIST'), findsOneWidget);
    expect(find.text('OTHER ARTIST'), findsNothing);
  });

  testWidgets('Ours shows group picks and reveals co-likers', (tester) async {
    const luke = ScheduleSupporter(userId: 'luke', displayName: 'Luke Smith');
    final sets = withScheduleClashes([
      set(id: 'mine', artist: 'MY PICK', start: 60, starred: true),
      set(
        id: 'group',
        artist: 'GROUP PICK',
        start: 180,
        likedByGroup: true,
        supporters: const [luke],
      ),
      set(id: 'other', artist: 'OTHER SET', start: 300),
    ]);
    final festival = Festival.fromJson({
      'id': 'fest',
      'name': 'Test Fest',
      'year': 2026,
      'location': 'Field',
      'city': 'City',
      'country': 'GB',
      'genres': <String>[],
      'stages': <Object>[],
    });

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FestivalDetailScreen(
            festival: festival,
            now: DateTime(2026),
            stages: const [
              Stage(id: 'main', name: 'Main', short: 'M', color: 0xFFFF2D8F),
            ],
            days: const [
              Day(
                id: 'fri',
                label: 'Friday',
                dayNum: '1',
                month: 'JUL',
                year: 2026,
              ),
            ],
            sets: sets,
          ),
        ),
      ),
    );

    await tester.tap(find.text('OURS'));
    await tester.tap(find.text('DAYS'));
    await tester.pumpAndSettle();

    expect(find.text('MY PICK'), findsOneWidget);
    expect(find.text('GROUP PICK'), findsOneWidget);
    expect(find.text('OTHER SET'), findsNothing);
    expect(find.text('LUKE'), findsOneWidget);

    await tester.tap(find.text('LUKE'));
    await tester.pumpAndSettle();
    expect(find.text('1 ALSO SAVED'), findsOneWidget);
    expect(find.text('Luke Smith'), findsOneWidget);
  });

  testWidgets(
    'member schedule resolves lineup details and marks missing sets',
    (tester) async {
      const lineup = LineupDto(
        stages: [
          LineupStageDto(
            id: 'main',
            name: 'Main Stage',
            short: 'MAIN',
            color: '#FF2D8F',
            order: 0,
          ),
        ],
        days: [
          LineupDayDto(
            id: 'fri',
            label: 'Friday',
            num: 1,
            month: 'JUL',
            year: 2026,
          ),
        ],
        sets: [
          LineupSetDto(
            id: 'known',
            day: 'fri',
            stage: 'main',
            artist: 'KNOWN ARTIST',
            startMin: 60,
            durationMin: 60,
            genre: 'test',
            cancelled: false,
          ),
        ],
      );
      const member = GroupMemberDto(
        userId: 'luke',
        displayName: 'Luke Smith',
        status: 'active',
        starredSetIds: ['known', 'missing'],
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Builder(
              builder: (context) => TextButton(
                onPressed: () => showModalBottomSheet<void>(
                  context: context,
                  isScrollControlled: true,
                  builder: (_) => const MemberSheet(
                    member: member,
                    groupName: 'Crew',
                    lineup: lineup,
                  ),
                ),
                child: const Text('OPEN'),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('OPEN'));
      await tester.pumpAndSettle();

      expect(find.text('KNOWN ARTIST'), findsOneWidget);
      expect(find.text('Friday · 01:00 · Main Stage'), findsOneWidget);
      expect(find.text('SET UNAVAILABLE · MISSING'), findsOneWidget);
    },
  );

  testWidgets('Clashes view shows overlaps without fake resolution actions', (
    tester,
  ) async {
    final sets = withScheduleClashes([
      set(id: 'a', artist: 'ARTIST A', start: 60, starred: true),
      set(id: 'b', artist: 'ARTIST B', start: 90, starred: true),
    ]);
    final festival = Festival.fromJson({
      'id': 'fest',
      'name': 'Test Fest',
      'year': 2026,
      'location': 'Field',
      'city': 'City',
      'country': 'GB',
      'genres': <String>[],
      'stages': <Object>[],
    });

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FestivalDetailScreen(
            festival: festival,
            now: DateTime(2026),
            stages: const [
              Stage(id: 'main', name: 'Main', short: 'M', color: 0xFFFF2D8F),
            ],
            days: const [
              Day(
                id: 'fri',
                label: 'Friday',
                dayNum: '1',
                month: 'JUL',
                year: 2026,
              ),
            ],
            sets: sets,
          ),
        ),
      ),
    );

    await tester.tap(find.text('CLASHES'));
    await tester.pumpAndSettle();

    expect(find.text('1 conflict'), findsOneWidget);
    expect(find.text('★ ARTIST A'), findsOneWidget);
    expect(find.text('★ ARTIST B'), findsOneWidget);
    expect(find.textContaining('SPLIT'), findsNothing);
    expect(find.textContaining('UNSTAR'), findsNothing);
  });
}
