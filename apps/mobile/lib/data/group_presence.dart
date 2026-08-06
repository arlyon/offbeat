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
    groupMemberLocationKey(member) != groupPresenceOfflineKey;

String groupMemberLocationLabel(
  GroupMemberDto member,
  Map<String, String> stages,
) {
  final key = groupMemberLocationKey(member);
  if (key == groupPresenceCampsiteKey) return 'CAMPSITE';
  if (key == groupPresenceOfflineKey) return 'OFF GRID';
  if (key.startsWith('stage:')) {
    final stageId = key.substring('stage:'.length);
    return (stages[stageId] ?? stageId).toUpperCase();
  }
  final custom = member.customLocation?.trim();
  return custom == null || custom.isEmpty ? 'OFF GRID' : custom.toUpperCase();
}
