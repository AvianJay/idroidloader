package app.idroidloader.mobile

import android.app.Activity
import android.content.Context
import android.net.wifi.WifiManager
import android.view.WindowManager
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class WirelessNetworkPlugin(private val hostActivity: Activity) : Plugin(hostActivity) {
    private val multicast = (hostActivity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
        .createMulticastLock("idroidloader-pairing").apply { setReferenceCounted(true) }
    private var users = 0

    @Command
    fun acquire(invoke: Invoke) = hostActivity.runOnUiThread {
        try {
            multicast.acquire()
            users++
            hostActivity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            invoke.resolve(JSObject())
        } catch (_: Exception) {
            invoke.reject("Unable to enable Wi-Fi multicast")
        }
    }

    @Command
    fun release(invoke: Invoke) = hostActivity.runOnUiThread {
        if (users > 0) {
            users--
            if (multicast.isHeld) multicast.release()
        }
        if (users == 0) hostActivity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        invoke.resolve(JSObject())
    }
}
