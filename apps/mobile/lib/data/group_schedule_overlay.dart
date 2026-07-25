import 'dart:async';

import 'package:flutter/foundation.dart';

import '../src/rust/api.dart';
import '../src/rust/api/dto.dart';

@immutable
class ScheduleSupporter {
  final String userId;
  final String displayName;

  const ScheduleSupporter({required this.userId, required this.displayName});
}

@immutable
class GroupScheduleOverlay {
  final Map<String, List<ScheduleSupporter>> supportersBySetId;
  final Map<String, Set<String>> likedSetIdsByUserId;
  final Set<String> groupLikedSetIds;

  const GroupScheduleOverlay({
    required this.supportersBySetId,
    required this.likedSetIdsByUserId,
    required this.groupLikedSetIds,
  });

  static const empty = GroupScheduleOverlay(
    supportersBySetId: {},
    likedSetIdsByUserId: {},
    groupLikedSetIds: {},
  );

  factory GroupScheduleOverlay.fromGroupStates({
    required Iterable<GroupStateDto> states,
    required String localUserId,
  }) {
    final members = <String, ScheduleSupporter>{};
    final likedByUser = <String, Set<String>>{};

    for (final state in states) {
      for (final member in state.members.where(
        (member) => member.status == 'active',
      )) {
        final existing = members[member.userId];
        final candidateName = member.displayName.trim();
        if (existing == null ||
            (candidateName.isNotEmpty &&
                candidateName.toLowerCase().compareTo(
                      existing.displayName.toLowerCase(),
                    ) <
                    0)) {
          members[member.userId] = ScheduleSupporter(
            userId: member.userId,
            displayName: candidateName.isEmpty ? 'anon' : candidateName,
          );
        }
        likedByUser
            .putIfAbsent(member.userId, () => <String>{})
            .addAll(member.starredSetIds);
      }
    }

    final supporters = <String, List<ScheduleSupporter>>{};
    for (final entry in likedByUser.entries) {
      if (entry.key == localUserId) continue;
      final supporter = members[entry.key];
      if (supporter == null) continue;
      for (final setId in entry.value) {
        supporters.putIfAbsent(setId, () => []).add(supporter);
      }
    }
    for (final values in supporters.values) {
      values.sort((a, b) {
        final byName = a.displayName.toLowerCase().compareTo(
          b.displayName.toLowerCase(),
        );
        return byName != 0 ? byName : a.userId.compareTo(b.userId);
      });
    }

    return GroupScheduleOverlay(
      supportersBySetId: Map<String, List<ScheduleSupporter>>.unmodifiable(
        supporters.map(
          (key, value) =>
              MapEntry(key, List<ScheduleSupporter>.unmodifiable(value)),
        ),
      ),
      likedSetIdsByUserId: Map<String, Set<String>>.unmodifiable(
        likedByUser.map(
          (key, value) => MapEntry(key, Set<String>.unmodifiable(value)),
        ),
      ),
      groupLikedSetIds: Set<String>.unmodifiable(
        likedByUser.values.expand((setIds) => setIds),
      ),
    );
  }
}

/// Owns one live subscription per local group for a festival and exposes a
/// single identity-deduplicated schedule overlay.
class GroupScheduleOverlayController extends ChangeNotifier {
  final AppNode node;
  final String festivalId;
  final String localUserId;

  final Map<String, GroupStateDto> _states = {};
  final Map<String, StreamSubscription<GroupStateDto>> _subscriptions = {};
  GroupScheduleOverlay _overlay = GroupScheduleOverlay.empty;
  int _refreshGeneration = 0;
  bool _disposed = false;

  GroupScheduleOverlayController({
    required this.node,
    required this.festivalId,
    required this.localUserId,
  });

  GroupScheduleOverlay get overlay => _overlay;

  Future<void> refresh() async {
    final generation = ++_refreshGeneration;
    final groups = await node.getGroups(festivalId: festivalId);
    if (_disposed || generation != _refreshGeneration) return;

    final groupIds = groups.map((group) => group.id).toSet();
    for (final removedId
        in _subscriptions.keys
            .where((groupId) => !groupIds.contains(groupId))
            .toList()) {
      await _subscriptions.remove(removedId)?.cancel();
      _states.remove(removedId);
    }

    for (final group in groups) {
      if (_subscriptions.containsKey(group.id)) continue;
      try {
        _states[group.id] = await node.getGroupState(groupId: group.id);
        final stream = await node.watchGroupState(groupId: group.id);
        if (_disposed || generation != _refreshGeneration) return;
        _subscriptions[group.id] = stream.listen((state) {
          if (_disposed) return;
          _states[group.id] = state;
          _rebuild();
        });
      } catch (error) {
        debugPrint('group schedule watch failed for ${group.id}: $error');
      }
    }
    _rebuild();
  }

  void _rebuild() {
    _overlay = GroupScheduleOverlay.fromGroupStates(
      states: _states.values,
      localUserId: localUserId,
    );
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _refreshGeneration++;
    for (final subscription in _subscriptions.values) {
      unawaited(subscription.cancel());
    }
    _subscriptions.clear();
    super.dispose();
  }
}
