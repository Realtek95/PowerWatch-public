package ua.powerwatch.app

import android.Manifest
import android.app.Activity
import app.tauri.plugin.Plugin
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Channel
import android.content.Intent

@InvokeArg
class ConfigureArgs { lateinit var databasePath: String; var onOpen: Channel? = null }
@InvokeArg
class InstallUpdateArgs { lateinit var path: String; var versionCode: Long = 0; lateinit var sha256: String }

@TauriPlugin(permissions = [Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications")])
class PowerWatchPlugin(private val activity: Activity) : Plugin(activity) {
    private var onOpen: Channel? = null
    init {
        (activity as? MainActivity)?.notificationPermissionChanged = {
            onOpen?.send(JSObject().put("notificationPermissionChanged",true))
        }
    }
    override fun onNewIntent(intent: Intent) {
        if(intent.getBooleanExtra("powerwatchSettings",false)) onOpen?.send(JSObject().put("settings",true))
        else if(intent.getBooleanExtra("powerwatchHome",false)) onOpen?.send(JSObject())
    }
    @Command
    fun launchAction(invoke: Invoke) {
        val settings=activity.intent.getBooleanExtra("powerwatchSettings",false)
        activity.intent.removeExtra("powerwatchSettings")
        invoke.resolve(JSObject().put("settings",settings))
    }
    @Command
    fun configure(invoke: Invoke) {
        val args = invoke.parseArgs(ConfigureArgs::class.java)
        onOpen = args.onOpen
        if (!activity.getSharedPreferences("powerwatch", 0).edit().putString("database", args.databasePath).commit()) {
            invoke.reject("storage_error")
            return
        }
        BackgroundBridge.configure(activity)
        invoke.resolve(JSObject())
    }
    @Command
    fun updateInfo(invoke: Invoke) {
        val info = activity.packageManager.getPackageInfo(activity.packageName, 0)
        invoke.resolve(JSObject().put("versionCode", info.longVersionCode))
    }
    @Command
    fun notificationStatus(invoke: Invoke) {
        invoke.resolve(JSObject().put("allowed", androidx.core.app.NotificationManagerCompat.from(activity).areNotificationsEnabled())
            .put("pending", (activity as? MainActivity)?.notificationPermissionPending == true))
    }
    @Command
    fun flushNotifications(invoke: Invoke) {
        Thread({
            try { BackgroundBridge.syncAlarms(activity.applicationContext); invoke.resolve(JSObject()) }
            catch (_: Exception) { invoke.reject("platform_error") }
        }, "PowerWatch-notifications").start()
    }
    @Command
    fun installUpdate(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(InstallUpdateArgs::class.java)
            val status = UpdateInstaller.install(activity, args)
            invoke.resolve(JSObject().put("status", status))
        } catch (_: Exception) { invoke.reject("update_signature") }
    }
}
