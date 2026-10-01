package app.idroidloader.mobile

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.util.concurrent.Executors

@InvokeArg
class SecureStorageArgs {
    lateinit var key: String
    var value: String? = null
}

@TauriPlugin
class SecureStoragePlugin(activity: Activity) : Plugin(activity) {
    private val store = EncryptedStore(activity.applicationContext)
    private val worker = Executors.newSingleThreadExecutor()

    private fun execute(invoke: Invoke, operation: () -> JSObject) {
        worker.execute {
            try {
                invoke.resolve(operation())
            } catch (_: Exception) {
                // Exception details must not include passwords, keys, or record contents.
                invoke.reject("Android encrypted storage operation failed")
            }
        }
    }

    @Command
    fun available(invoke: Invoke) = execute(invoke) {
        JSObject().apply { put("available", store.available()) }
    }

    @Command
    fun store(invoke: Invoke) = execute(invoke) {
        val args = invoke.parseArgs(SecureStorageArgs::class.java)
        store.store(args.key, requireNotNull(args.value))
        JSObject()
    }

    @Command
    fun retrieve(invoke: Invoke) = execute(invoke) {
        val args = invoke.parseArgs(SecureStorageArgs::class.java)
        JSObject().apply { put("value", store.retrieve(args.key) ?: JSONObject.NULL) }
    }

    @Command
    fun delete(invoke: Invoke) = execute(invoke) {
        val args = invoke.parseArgs(SecureStorageArgs::class.java)
        JSObject().apply { put("removed", store.delete(args.key)) }
    }
}
