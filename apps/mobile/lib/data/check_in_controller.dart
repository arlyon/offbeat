import 'dart:async';

import 'package:flutter/foundation.dart';

import '../src/rust/api.dart';
import '../src/rust/api/dto.dart';

class CheckInController extends ChangeNotifier {
  final AppNode node;
  final String festivalId;

  FestivalCheckInDto? _checkIn;
  bool _loading = true;
  bool _saving = false;
  String? _error;
  int _groupCount = 0;
  Timer? _expiryTimer;

  CheckInController({required this.node, required this.festivalId});

  FestivalCheckInDto? get checkIn => _checkIn;
  bool get loading => _loading;
  bool get saving => _saving;
  String? get error => _error;
  int get groupCount => _groupCount;

  bool isAtStage(String stageId) =>
      _checkIn?.kind == 'stage' && _checkIn?.value == stageId;
  bool get isAtCampsite => _checkIn?.kind == 'campsite';

  Future<void> load() async {
    _loading = true;
    notifyListeners();
    try {
      _checkIn = await node.getFestivalCheckIn(festivalId: festivalId);
      _groupCount = (await node.getGroups(festivalId: festivalId)).length;
      _scheduleExpiry();
      _error = null;
    } catch (_) {
      _error = 'COULD NOT LOAD CHECK-IN';
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<bool> setStage(String stageId) => _set('stage', stageId);
  Future<bool> setCampsite() => _set('campsite', null);
  Future<bool> setCustom(String label) => _set('custom', label);
  Future<bool> clear() => _set('none', null);
  Future<bool> refresh() {
    final current = _checkIn;
    return current == null
        ? Future.value(true)
        : _set(current.kind, current.value);
  }

  void _scheduleExpiry() {
    _expiryTimer?.cancel();
    final checkIn = _checkIn;
    if (checkIn == null) return;
    final expiry = DateTime.fromMillisecondsSinceEpoch(
      checkIn.expiresAt * 1000,
    );
    final delay = expiry.difference(DateTime.now());
    if (delay <= Duration.zero) {
      unawaited(clear());
    } else {
      _expiryTimer = Timer(delay, () => unawaited(clear()));
    }
  }

  @override
  void dispose() {
    _expiryTimer?.cancel();
    super.dispose();
  }

  Future<bool> _set(String kind, String? value) async {
    if (_saving) return false;
    _saving = true;
    _error = null;
    notifyListeners();
    try {
      final result = await node.setFestivalCheckIn(
        festivalId: festivalId,
        kind: kind,
        value: value,
      );
      _checkIn = kind == 'none' ? null : result;
      _groupCount = (await node.getGroups(festivalId: festivalId)).length;
      _scheduleExpiry();
      return true;
    } catch (_) {
      _error = 'COULD NOT UPDATE CHECK-IN';
      return false;
    } finally {
      _saving = false;
      notifyListeners();
    }
  }
}
