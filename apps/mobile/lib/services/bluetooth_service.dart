// BT adapter state + runtime permission management via platform channel.
// Does not manage BLE connections — that's handled by iroh/blew in Rust.

import 'dart:io';
import 'package:flutter/services.dart';
import 'package:permission_handler/permission_handler.dart';

enum BluetoothState { on, off, permissionDenied, unknown }

class BluetoothService {
  static const _channel = MethodChannel('com.offbeat/bluetooth');

  /// Check if the BT adapter is currently powered on.
  static Future<BluetoothState> getState() async {
    // Check runtime permissions first (Android 12+).
    if (Platform.isAndroid) {
      final granted = await arePermissionsGranted();
      if (!granted) return BluetoothState.permissionDenied;
    }

    try {
      final result = await _channel.invokeMethod<bool>('isEnabled');
      if (result == null) return BluetoothState.unknown;
      return result ? BluetoothState.on : BluetoothState.off;
    } on MissingPluginException {
      return BluetoothState.unknown;
    }
  }

  /// Check if all required BLE runtime permissions are granted.
  static Future<bool> arePermissionsGranted() async {
    final scan = await Permission.bluetoothScan.isGranted;
    final connect = await Permission.bluetoothConnect.isGranted;
    final advertise = await Permission.bluetoothAdvertise.isGranted;
    return scan && connect && advertise;
  }

  /// Request BLE runtime permissions. Returns true if all granted.
  static Future<bool> requestPermissions() async {
    final statuses = await [
      Permission.bluetoothScan,
      Permission.bluetoothConnect,
      Permission.bluetoothAdvertise,
    ].request();
    return statuses.values.every((s) => s.isGranted);
  }

  /// Open the OS Bluetooth settings page.
  static Future<void> openSettings() async {
    try {
      await _channel.invokeMethod<void>('openSettings');
    } on MissingPluginException {
      // Not implemented on this platform
    }
  }

  /// Initialize the native BLE managers (Android only).
  /// Must be called after the Rust native library is loaded.
  static Future<void> initBle() async {
    try {
      await _channel.invokeMethod<void>('initBle');
    } on MissingPluginException {
      // Not implemented on this platform (iOS doesn't need this)
    }
  }
}
