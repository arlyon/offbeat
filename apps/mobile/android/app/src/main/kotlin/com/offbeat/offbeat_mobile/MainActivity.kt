package com.offbeat.offbeat_mobile

import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import androidx.annotation.NonNull
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import org.jakebot.blew.BleCentralManager
import org.jakebot.blew.BlePeripheralManager

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(@NonNull flutterEngine: FlutterEngine) {
        flutterEngine.plugins.add(BlewPlugin())
        super.configureFlutterEngine(flutterEngine)

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "com.offbeat/bluetooth")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "isEnabled" -> {
                        val bm = getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
                        result.success(bm?.adapter?.isEnabled == true)
                    }
                    "openSettings" -> {
                        startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS))
                        result.success(null)
                    }
                    "initBle" -> {
                        android.util.Log.i("BlewPlugin", "initBle: initializing BLE managers")
                        BleCentralManager.init(applicationContext)
                        BlePeripheralManager.init(applicationContext)
                        MeshtasticSidecarBridge.init(applicationContext)
                        val perms = BlePeripheralManager.areBlePermissionsGranted()
                        android.util.Log.i("BlewPlugin", "initBle: done, perms=$perms")
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
    }
}

/**
 * Passes applicationContext to Rust so blew can initialize its JNI layer
 * (caching BleCentralManager / BlePeripheralManager class refs and
 * registering the nativeOn* callbacks).
 *
 * Follows the flutter_rust_bridge "Method 2" NDK-init pattern.
 */
class BlewPlugin : FlutterPlugin, MethodChannel.MethodCallHandler {
    companion object {
        init {
            System.loadLibrary("offbeat_bridge")
        }
    }

    external fun init_android(ctx: Context)

    override fun onAttachedToEngine(@NonNull binding: FlutterPlugin.FlutterPluginBinding) {
        val ctx = binding.applicationContext
        android.util.Log.i("BlewPlugin", "onAttachedToEngine, initializing BLE managers")
        BleCentralManager.init(ctx)
        BlePeripheralManager.init(ctx)
        MeshtasticSidecarBridge.init(ctx)
        android.util.Log.i("BlewPlugin", "onAttachedToEngine, calling init_android")
        init_android(ctx)
        android.util.Log.i("BlewPlugin", "init_android returned")
    }

    override fun onMethodCall(@NonNull call: MethodCall, @NonNull result: MethodChannel.Result) {
        result.notImplemented()
    }

    override fun onDetachedFromEngine(@NonNull binding: FlutterPlugin.FlutterPluginBinding) {
    }
}
