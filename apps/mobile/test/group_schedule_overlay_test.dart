import 'package:flutter_test/flutter_test.dart';
import 'package:offbeat_mobile/data/group_schedule_overlay.dart';
import 'package:offbeat_mobile/src/rust/api/dto.dart';

GroupMemberDto member(
  String id,
  String name,
  List<String> stars, {
  String status = 'active',
}) => GroupMemberDto(
  userId: id,
  displayName: name,
  status: status,
  locationKind: 'none',
  starredSetIds: stars,
);

GroupStateDto group(String name, List<GroupMemberDto> members) =>
    GroupStateDto(name: name, members: members, pins: const []);

void main() {
  test('aggregates all groups and excludes self from supporter pins', () {
    final overlay = GroupScheduleOverlay.fromGroupStates(
      localUserId: 'me',
      states: [
        group('Crew', [
          member('me', 'Alex', ['set-a']),
          member('luke', 'Luke', ['set-a', 'set-b']),
        ]),
        group('Camp', [
          member('ornella', 'Ornella', ['set-c']),
        ]),
      ],
    );

    expect(overlay.groupLikedSetIds, {'set-a', 'set-b', 'set-c'});
    expect(
      overlay.supportersBySetId['set-a']!.map((person) => person.displayName),
      ['Luke'],
    );
    expect(
      overlay.supportersBySetId['set-c']!.map((person) => person.displayName),
      ['Ornella'],
    );
  });

  test('deduplicates the same person across groups by user identity', () {
    final overlay = GroupScheduleOverlay.fromGroupStates(
      localUserId: 'me',
      states: [
        group('Crew', [
          member('luke', 'Luke', ['set-a']),
        ]),
        group('Camp', [
          member('luke', 'Luke', ['set-a', 'set-b']),
        ]),
      ],
    );

    expect(overlay.supportersBySetId['set-a'], hasLength(1));
    expect(overlay.likedSetIdsByUserId['luke'], {'set-a', 'set-b'});
  });

  test(
    'includes offline members, excludes departed entries, and sorts names',
    () {
      final overlay = GroupScheduleOverlay.fromGroupStates(
        localUserId: 'me',
        states: [
          group('Crew', [
            member('zoe', 'Zoe', ['set-a'], status: 'offline'),
            member('ali', 'Ali', ['set-a']),
            member('gone', 'Gone', ['set-a'], status: 'left'),
          ]),
        ],
      );

      expect(
        overlay.supportersBySetId['set-a']!.map((person) => person.displayName),
        ['Ali', 'Zoe'],
      );
      expect(overlay.likedSetIdsByUserId, isNot(contains('gone')));
    },
  );
}
