package com.offbeat.offbeat_mobile

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.aware.WifiAwareManager
import android.os.Build

/**
 * Android Wi-Fi Aware/NAN capability probe.
 *
 * Full route wiring still needs publish/subscribe + NetworkSpecifier/NDP setup,
 * then either native IP socket hints for iroh or a custom transport adapter.
 */
object WifiAwareBridge {
    const val SERVICE_NAME = "_offbeat-sync._udp"

    data class Capability(
        val osSupported: Boolean,
        val hardwareSupported: Boolean,
        val nearbyWifiPermissionGranted: Boolean,
        val locationPermissionGranted: Boolean,
    ) {
        val potentiallyAvailable: Boolean
            get() = osSupported && hardwareSupported && nearbyWifiPermissionGranted && locationPermissionGranted
    }

    fun capability(context: Context): Capability {
        val osSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
        val hardwareSupported = if (osSupported) {
            context.packageManager.hasSystemFeature(PackageManager.FEATURE_WIFI_AWARE)
        } else {
            false
        }
        val nearbyWifiGranted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.checkSelfPermission(Manifest.permission.NEARBY_WIFI_DEVICES) ==
                PackageManager.PERMISSION_GRANTED
        } else {
            true
        }
        val locationGranted = context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) ==
            PackageManager.PERMISSION_GRANTED
        return Capability(osSupported, hardwareSupported, nearbyWifiGranted, locationGranted)
    }

    fun manager(context: Context): WifiAwareManager? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return null
        return context.getSystemService(WifiAwareManager::class.java)
    }
}
