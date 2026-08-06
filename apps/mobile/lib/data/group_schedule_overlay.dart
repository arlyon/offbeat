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
        (member) => member.status != 'left',
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
  final Map<String, Object> _subscriptionTokens = {};
  GroupScheduleOverlay _overlay = GroupScheduleOverlay.empty;
  int _refreshGeneration = 0;
  Timer? _retryTimer;
  Duration _retryDelay = const Duration(seconds: 1);
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
        in _states.keys
            .where((groupId) => !groupIds.contains(groupId))
            .toList()) {
      _subscriptionTokens.remove(removedId);
      await _subscriptions.remove(removedId)?.cancel();
      _states.remove(removedId);
    }

    for (final group in groups) {
      if (_subscriptions.containsKey(group.id)) continue;
      try {
        final state = await node.getGroupState(groupId: group.id);
        if (_disposed || generation != _refreshGeneration) return;
        final stream = await node.watchGroupState(groupId: group.id);
        if (_disposed || generation != _refreshGeneration) {
          await stream.listen((_) {}).cancel();
          return;
        }
        _states[group.id] = state;
        final token = Object();
        _subscriptionTokens[group.id] = token;
        final subscription = stream.listen(
          (state) {
            if (_disposed || !identical(_subscriptionTokens[group.id], token)) {
              return;
            }
            _states[group.id] = state;
            _rebuild();
          },
          onError: (_) => _handleWatchEnded(group.id, token),
          onDone: () => _handleWatchEnded(group.id, token),
          cancelOnError: true,
        );
        if (identical(_subscriptionTokens[group.id], token)) {
          _subscriptions[group.id] = subscription;
          _retryDelay = const Duration(seconds: 1);
        } else {
          await subscription.cancel();
        }
      } catch (_) {
        debugPrint('group schedule watch failed; retrying');
        _scheduleRetry();
      }
    }
    _rebuild();
  }

  void _handleWatchEnded(String groupId, Object token) {
    if (_disposed || !identical(_subscriptionTokens[groupId], token)) return;
    _subscriptionTokens.remove(groupId);
    _subscriptions.remove(groupId);
    _states.remove(groupId);
    _rebuild();
    _scheduleRetry();
  }

  void _scheduleRetry() {
    if (_disposed || _retryTimer?.isActive == true) return;
    final delay = _retryDelay;
    _retryDelay = Duration(seconds: (_retryDelay.inSeconds * 2).clamp(1, 30));
    _retryTimer = Timer(delay, () {
      if (_disposed) return;
      unawaited(refresh().catchError((_) {}));
    });
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
    _retryTimer?.cancel();
    for (final subscription in _subscriptions.values) {
      unawaited(subscription.cancel());
    }
    _subscriptions.clear();
    _subscriptionTokens.clear();
    super.dispose();
  }
}
