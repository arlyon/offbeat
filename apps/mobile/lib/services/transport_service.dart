import 'dart:async';
import '../src/rust/api.dart';
import '../src/rust/api/dto.dart';

/// Wraps transport status from the Rust core (relay + BLE).
class TransportService {
  final AppNode _node;

  TransportService(this._node);

  /// Get a one-shot snapshot of transport status.
  Future<TransportStatusDto> getStatus() {
    return _node.getTransportStatus();
  }

  /// Stream of transport status updates (emits every second with
  /// bandwidth rates computed by diffing cumulative byte counters).
  Future<Stream<TransportStatusDto>> watchStatus() {
    return _node.watchTransportStatus();
  }
}
