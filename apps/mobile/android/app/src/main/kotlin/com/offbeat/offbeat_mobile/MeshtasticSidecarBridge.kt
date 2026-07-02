package com.offbeat.offbeat_mobile

import android.bluetooth.BluetoothGattCharacteristic
import android.content.Context
import org.jakebot.blew.BleCentralManager

/**
 * Android bridge for a Bluetooth-paired Meshtastic sidecar.
 *
 * This is intentionally thin: Rust owns Offbeat packetization, queueing,
 * reassembly, dedupe, and lifecycle policy. The platform bridge performs BLE
 * discovery/GATT operations against Meshtastic's standard service. The official
 * mobile apps subscribe to FromNum notifications and drain pending FromRadio
 * protobufs by repeatedly reading the FromRadio characteristic.
 */
object MeshtasticSidecarBridge {
    const val SERVICE_UUID = "6ba1b218-15a8-461f-9fa8-5dcae273eafd"
    const val TO_RADIO_CHAR_UUID = "f75c76d2-129e-4dad-a1dd-7866124401e7"
    const val FROM_NUM_CHAR_UUID = "ed9da18c-a800-4f66-a670-aa7547e34453"
    const val FROM_RADIO_CHAR_UUID = "2c55e69e-4993-11ed-b878-0242ac120002"

    fun init(context: Context) {
        BleCentralManager.init(context.applicationContext)
    }

    fun startScan() {
        BleCentralManager.startScan(arrayOf(SERVICE_UUID), lowPower = true)
    }

    fun stopScan() {
        BleCentralManager.stopScan()
    }

    /**
     * Connect to the selected sidecar. Rust should call discoverAndSubscribe
     * after the connection callback reports services are ready.
     */
    fun connect(deviceAddress: String) {
        BleCentralManager.connect(deviceAddress)
    }

    fun disconnect(deviceAddress: String) {
        BleCentralManager.forceClose(deviceAddress)
    }

    fun discoverServices(deviceAddress: String): Int = BleCentralManager.discoverServices(deviceAddress)

    /**
     * Subscribe to Meshtastic FromNum notifications; each notification should trigger drain reads.
     */
    fun subscribeFromNum(deviceAddress: String): Int = BleCentralManager.subscribeCharacteristic(deviceAddress, FROM_NUM_CHAR_UUID)

    /** Read one pending Meshtastic FromRadio protobuf. Empty data means the drain is complete. */
    fun readFromRadio(deviceAddress: String): Int = BleCentralManager.readCharacteristic(deviceAddress, FROM_RADIO_CHAR_UUID)

    /**
     * Optional diagnostic/compatibility subscription; FromNum remains the primary drain trigger.
     */
    fun subscribeFromRadio(deviceAddress: String): Int = BleCentralManager.subscribeCharacteristic(deviceAddress, FROM_RADIO_CHAR_UUID)

    /**
     * Write a Meshtastic ToRadio protobuf containing an Offbeat PRIVATE_APP
     * payload. Rust is responsible for protobuf serialization and portnum=256.
     */
    fun writeToRadio(
        deviceAddress: String,
        toRadioProtobuf: ByteArray,
    ): Int =
        BleCentralManager.writeCharacteristic(
            deviceAddress,
            TO_RADIO_CHAR_UUID,
            toRadioProtobuf,
            BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT,
        )
}
