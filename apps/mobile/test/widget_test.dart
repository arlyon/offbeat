import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/group_presence.dart';
import 'package:offbeat_mobile/data/group_schedule_overlay.dart';
import 'package:offbeat_mobile/data/models.dart';
import 'package:offbeat_mobile/data/serial_keyed_queue.dart';
import 'package:offbeat_mobile/screens/festival_detail/festival_detail_screen.dart';
import 'package:offbeat_mobile/screens/festival_detail/lineup_search_screen.dart';
import 'package:offbeat_mobile/screens/festival_detail/set_details_sheet.dart';
import 'package:offbeat_mobile/screens/social/group_members_sheet.dart';
import 'package:offbeat_mobile/screens/social/member_sheet.dart';
import 'package:offbeat_mobile/src/rust/api/dto.dart';
import 'package:offbeat_mobile/widgets/co_liker_pins.dart';

FestSet set({
  required String id,
  required String artist,
  required int start,
  bool starred = false,
  String day = 'fri',
  String stage = 'main',
  bool cancelled = false,
  bool likedByGroup = false,
  List<ScheduleSupporter> supporters = const [],
  List<String> clashes = const [],
}) => FestSet(
  id: id,
  day: day,
  stage: stage,
  artist: artist,
  t: start,
  dur: 60,
  genre: 'test',
  starred: starred,
  cancelled: cancelled,
  likedByGroup: likedByGroup,
  supporters: supporters,
  clashes: clashes,
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

  testWidgets('Mine filter shows only liked sets', (tester) async {
    final sets = withScheduleClashes([
      set(id: 'liked', artist: 'LIKED ARTIST', start: 60, starred: true),
      set(id: 'other', artist: 'OTHER ARTIST', start: 180),
    ]);

    await tester.pumpWidget(
      MaterialApp(
        home: LineupSearchScreen(
          sets: sets,
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
          onSetTap: (_) {},
        ),
      ),
    );

    await tester.tap(find.bySemanticsLabel('Open lineup filters'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('MINE'));
    await tester.pump();
    await tester.tap(find.textContaining('SHOW ').last);
    await tester.pumpAndSettle();

    expect(find.text('LIKED ARTIST'), findsOneWidget);
    expect(find.text('OTHER ARTIST'), findsNothing);
  });

  testWidgets('Ours filter includes local and group picks', (tester) async {
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

    await tester.pumpWidget(
      MaterialApp(
        home: LineupSearchScreen(
          sets: sets,
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
          onSetTap: (_) {},
        ),
      ),
    );

    await tester.tap(find.bySemanticsLabel('Open lineup filters'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('OURS'));
    await tester.pump();
    await tester.tap(find.textContaining('SHOW ').last);
    await tester.pumpAndSettle();

    expect(find.text('MY PICK'), findsOneWidget);
    expect(find.text('GROUP PICK'), findsOneWidget);
    expect(find.text('OTHER SET'), findsNothing);
  });

  testWidgets('set drawer shows offline artist path and connections', (
    tester,
  ) async {
    const luke = ScheduleSupporter(userId: 'luke', displayName: 'Luke Smith');
    final first = set(
      id: 'artist-fri',
      artist: 'The Artist',
      start: 60,
      starred: true,
      supporters: const [luke],
      clashes: const ['clash'],
    );
    final second = set(
      id: 'artist-sat',
      artist: '  the   artist ',
      start: 180,
      day: 'sat',
      stage: 'quarry',
      supporters: const [luke],
    );
    final clash = set(id: 'clash', artist: 'OTHER ACT', start: 90);
    final allSets = [first, second, clash];

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () => showSetDetailsSheet(
                context,
                set: first,
                stages: const [
                  Stage(
                    id: 'main',
                    name: 'Main',
                    short: 'M',
                    color: 0xFFFF2D8F,
                  ),
                  Stage(
                    id: 'quarry',
                    name: 'The Quarry',
                    short: 'Q',
                    color: 0xFF4CC9F0,
                  ),
                ],
                days: const [
                  Day(
                    id: 'fri',
                    label: 'Friday',
                    dayNum: '1',
                    month: 'JUL',
                    year: 2026,
                  ),
                  Day(
                    id: 'sat',
                    label: 'Saturday',
                    dayNum: '2',
                    month: 'JUL',
                    year: 2026,
                  ),
                ],
                allSets: allSets,
              ),
              child: const Text('OPEN'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('OPEN'));
    await tester.pumpAndSettle();

    expect(find.text('ARTIST PATH'), findsOneWidget);
    expect(find.text('2 APPEARANCES · 120 MIN'), findsOneWidget);
    expect(find.byIcon(Icons.close), findsNothing);
    expect(find.text('LIKED'), findsNothing);
    expect(find.text('CLASH'), findsNothing);

    await tester.scrollUntilVisible(
      find.text('EXPAND CONNECTIONS'),
      160,
      scrollable: find.byType(Scrollable).last,
    );
    await tester.tap(find.text('EXPAND CONNECTIONS'));
    await tester.pumpAndSettle();
    expect(find.text('FRIEND OVERLAP'), findsOneWidget);
    expect(find.text('CLASH LINKS'), findsOneWidget);
  });

  testWidgets('group members drawer filters presence and reveals all', (
    tester,
  ) async {
    const members = [
      GroupMemberDto(
        userId: 'me',
        displayName: 'Me Person',
        status: 'active',
        locationKind: 'stage',
        stageId: 'main',
        starredSetIds: ['a'],
      ),
      GroupMemberDto(
        userId: 'luke',
        displayName: 'Luke Smith',
        status: 'offline',
        locationKind: 'none',
        starredSetIds: [],
      ),
      GroupMemberDto(
        userId: 'ali',
        displayName: 'Ali Jones',
        status: 'active',
        locationKind: 'stage',
        stageId: 'quarry',
        starredSetIds: ['b', 'c'],
      ),
      GroupMemberDto(
        userId: 'sam',
        displayName: 'Sam Campsite',
        status: 'active',
        locationKind: 'campsite',
        customLocation: 'Campsite',
        updatedAt: '2026-08-05T14:30:00Z',
        starredSetIds: [],
      ),
      GroupMemberDto(
        userId: 'zoe',
        displayName: 'Zoe Campsite',
        status: 'stale',
        locationKind: 'campsite',
        customLocation: 'Campsite',
        updatedAt: '2026-08-05T09:15:00Z',
        starredSetIds: [],
      ),
    ];

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: GroupMembersSheet(
            members: members,
            stages: const {'main': 'Main Stage', 'quarry': 'The Quarry'},
            userId: 'me',
            initialLocationKey: 'stage:main',
            onMemberTap: (_) {},
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('YOU'), findsOneWidget);
    expect(find.text('Luke Smith'), findsNothing);
    expect(find.text('Ali Jones'), findsNothing);
    expect(find.text('FILTERED · MAIN STAGE'), findsOneWidget);

    await tester.tap(find.text('SHOW ALL ×'));
    await tester.pumpAndSettle();
    expect(find.text('Luke Smith'), findsOneWidget);
    expect(find.text('Ali Jones'), findsOneWidget);
    expect(find.text('Sam Campsite'), findsOneWidget);
    expect(find.text('Zoe Campsite'), findsOneWidget);
    expect(
      find.text('CAMPSITE · ${groupMemberCheckInTime(members[3])}'),
      findsOneWidget,
    );
    expect(
      find.text('CAMPSITE · STALE · ${groupMemberCheckInTime(members[4])}'),
      findsOneWidget,
    );
    expect(find.text('NO CHECK-IN YET'), findsOneWidget);
    expect(find.text('3 ON SITE · 5 TOTAL'), findsOneWidget);
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
        status: 'stale',
        locationKind: 'campsite',
        customLocation: 'Campsite',
        updatedAt: '2026-08-05T09:15:00Z',
        expiresAt: '2026-08-05T13:15:00Z',
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
      expect(
        find.text('CAMPSITE · STALE · ${groupMemberCheckInTime(member)}'),
        findsOneWidget,
      );
      expect(find.text('DM'), findsNothing);
      expect(find.text('LOCATE'), findsNothing);
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
            initialView: FestDetailView.clashRadar,
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

    await tester.pumpAndSettle();

    expect(find.text('LIKED'), findsOneWidget);
    expect(find.text('1 conflict'), findsOneWidget);
    expect(find.text('★ ARTIST A'), findsOneWidget);
    expect(find.text('★ ARTIST B'), findsOneWidget);
    expect(find.textContaining('SPLIT'), findsNothing);
    expect(find.textContaining('UNSTAR'), findsNothing);
  });
}
