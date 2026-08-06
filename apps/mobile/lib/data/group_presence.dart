import '../src/rust/api/dto.dart';

const groupPresenceCampsiteKey = 'campsite';
const groupPresenceOfflineKey = 'offline';

String groupMemberLocationKey(GroupMemberDto member) {
  if (member.locationKind == 'stage') {
    final stageId = member.stageId?.trim();
    if (stageId != null && stageId.isNotEmpty) return 'stage:$stageId';
  }
  if (member.locationKind == 'campsite') return groupPresenceCampsiteKey;
  if (member.locationKind == 'custom') {
    final location = member.customLocation?.trim();
    if (location != null && location.isNotEmpty) {
      return 'custom:${location.toLowerCase()}';
    }
  }

  // Compatibility with group documents created before locationKind existed.
  final stageId = member.stageId?.trim();
  if (stageId != null && stageId.isNotEmpty) return 'stage:$stageId';
  final location = member.customLocation?.trim();
  if (location != null && location.isNotEmpty) {
    if (location.toLowerCase() == 'campsite') return groupPresenceCampsiteKey;
    return 'custom:${location.toLowerCase()}';
  }
  return groupPresenceOfflineKey;
}

bool groupMemberIsOnSite(GroupMemberDto member) =>
    member.status != 'offline' &&
    !groupMemberIsStale(member) &&
    groupMemberLocationKey(member) != groupPresenceOfflineKey;

bool groupMemberIsStale(GroupMemberDto member) {
  if (groupMemberLocationKey(member) == groupPresenceOfflineKey) return false;
  if (member.status == 'stale' || member.status == 'offline') return true;
  final expiry = _parseCheckInTimestamp(member.expiresAt);
  return expiry != null && !expiry.isAfter(DateTime.now());
}

String groupMemberLocationLabel(
  GroupMemberDto member,
  Map<String, String> stages,
) {
  final key = groupMemberLocationKey(member);
  if (key == groupPresenceCampsiteKey) return 'CAMPSITE';
  if (key == groupPresenceOfflineKey) return 'NO CHECK-IN YET';
  if (key.startsWith('stage:')) {
    final stageId = key.substring('stage:'.length);
    return (stages[stageId] ?? stageId).toUpperCase();
  }
  final custom = member.customLocation?.trim();
  return custom == null || custom.isEmpty
      ? 'NO CHECK-IN YET'
      : custom.toUpperCase();
}

DateTime? _parseCheckInTimestamp(String? raw) {
  if (raw == null || raw.isEmpty) return null;
  final epochSeconds = int.tryParse(
    raw.endsWith('Z') ? raw.substring(0, raw.length - 1) : raw,
  );
  if (epochSeconds != null) {
    return DateTime.fromMillisecondsSinceEpoch(epochSeconds * 1000);
  }
  return DateTime.tryParse(raw)?.toLocal();
}

String groupMemberCheckInTime(GroupMemberDto member) {
  final timestamp = _parseCheckInTimestamp(member.updatedAt);
  if (timestamp == null) return '';
  final hour = timestamp.hour.toString().padLeft(2, '0');
  final minute = timestamp.minute.toString().padLeft(2, '0');
  return '$hour:$minute';
}

String groupMemberPresenceLabel(
  GroupMemberDto member,
  Map<String, String> stages,
) {
  final location = groupMemberLocationLabel(member, stages);
  if (groupMemberLocationKey(member) == groupPresenceOfflineKey) {
    return location;
  }
  final time = groupMemberCheckInTime(member);
  final freshness = groupMemberIsStale(member) ? 'STALE' : null;
  return [
    location,
    ?freshness,
    if (time.isNotEmpty) time,
  ].join(' · ');
}
