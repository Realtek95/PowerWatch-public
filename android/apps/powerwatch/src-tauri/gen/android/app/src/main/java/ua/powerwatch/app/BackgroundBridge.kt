package ua.powerwatch.app

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.*
import org.json.JSONObject
import java.time.Instant
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/** OS adapter only: requests, freshness, quiet hours and claims are Rust decisions. */
object BackgroundBridge {
    init { System.loadLibrary("powerwatch_app_lib") }
    @JvmStatic external fun nativeCall(database: String, operation: String, payload: String, wall: Long): String
    private val executor = Executors.newSingleThreadScheduledExecutor()
    private var foreground: ScheduledFuture<*>? = null
    @Volatile private var visible = false
    private fun database(context: Context) = context.getSharedPreferences("powerwatch", 0).getString("database", null)
    fun call(context: Context, operation: String, payload: JSONObject = JSONObject()): Any? {
        val path = database(context) ?: return null
        val reply = JSONObject(nativeCall(path, operation, payload.toString(), System.currentTimeMillis()))
        check(reply.optBoolean("ok")) { "Rust local repository failed" }
        return reply.opt("value").takeUnless { it == JSONObject.NULL }
    }
    fun configure(context: Context) {
        val app = context.applicationContext
        PowerWatchWidget.refreshDefinitions(app)
        val periodic = PeriodicWorkRequestBuilder<PulseWorker>(15, TimeUnit.MINUTES).build()
        WorkManager.getInstance(app).enqueueUniquePeriodicWork("powerwatch-pulse", ExistingPeriodicWorkPolicy.KEEP, periodic)
        enqueueLocal(app, ExistingWorkPolicy.KEEP)
        if (visible) startTicker(app)
    }
    @Synchronized fun startForeground(context: Context) { visible = true; startTicker(context) }
    private fun startTicker(context: Context) {
        if (database(context) == null || foreground != null) return
        foreground = executor.scheduleWithFixedDelay({
            try { run(context.applicationContext) } catch (error: Exception) { Log.e("PowerWatch", "Local foreground tick failed", error) }
        }, 0, 2, TimeUnit.SECONDS)
    }
    @Synchronized fun stopForeground() { visible = false; foreground?.cancel(false); foreground = null }
    internal fun pollingConstraints() = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
    private fun networkReady(context: Context): Boolean {
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val capabilities = connectivity.getNetworkCapabilities(connectivity.activeNetwork) ?: return false
        if (!visible && connectivity.isActiveNetworkMetered && connectivity.restrictBackgroundStatus == ConnectivityManager.RESTRICT_BACKGROUND_STATUS_ENABLED) return false
        return capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    }
    fun enqueueLocal(context: Context, policy: ExistingWorkPolicy) {
        val request = OneTimeWorkRequestBuilder<PulseWorker>().build()
        WorkManager.getInstance(context).enqueueUniqueWork("powerwatch-local-pulse", policy, request)
    }
    fun enqueue(context: Context, delay: Long, policy: ExistingWorkPolicy) {
        val request = OneTimeWorkRequestBuilder<MonitorWorker>().setConstraints(pollingConstraints()).setInitialDelay(delay.coerceAtLeast(0), TimeUnit.SECONDS).build()
        WorkManager.getInstance(context).enqueueUniqueWork("powerwatch-monitor", policy, request)
    }
    fun run(context: Context, networkAvailable: Boolean = networkReady(context)): Long {
        // Deliver still-relevant local alarms before any potentially slow HTTP.
        syncAlarms(context)
        if (!networkAvailable) {
            PowerWatchWidget.updateAll(context)
            return 15
        }
        val tick = call(context, "tick") as? JSONObject ?: return 900
        syncAlarms(context)
        PowerWatchWidget.updateAll(context)
        return tick.optLong("nextDelaySeconds", 900).coerceAtLeast(5)
    }
    private fun intent(context: Context, key: String) = Intent(context, ReminderReceiver::class.java).setData(Uri.parse("powerwatch://reminder/${Uri.encode(key)}")).putExtra("key", key)
    @Synchronized fun syncAlarms(context: Context) {
        val value = call(context, "pending") as? JSONObject ?: return
        val now = Instant.parse(value.getString("now")).toEpochMilli()
        val records = value.getJSONArray("notifications")
        val keys = mutableSetOf<String>()
        val manager = context.getSystemService(AlarmManager::class.java)
        for (index in 0 until records.length()) {
            val plan = records.getJSONObject(index).getJSONObject("plan")
            val key = plan.getString("key"); keys.add(key)
            val pending = PendingIntent.getBroadcast(context, 0, intent(context,key), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val remaining = Instant.parse(plan.getString("at")).toEpochMilli() - now
            if (remaining <= 0) { deliver(context, key); continue }
            // Inexact alarms need no privileged exact-alarm permission. Doze can delay them.
            manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, System.currentTimeMillis() + remaining.coerceAtLeast(100), pending)
        }
        val preferences = context.getSharedPreferences("powerwatch", 0)
        for (old in preferences.getStringSet("alarms", emptySet())!!.minus(keys)) {
            val pending = PendingIntent.getBroadcast(context, 0, intent(context,old), PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE)
            if (pending != null) { manager.cancel(pending); pending.cancel() }
        }
        preferences.edit().putStringSet("alarms", keys).apply()
    }
    @Synchronized fun deliver(context: Context, key: String) {
        val permitted = NotificationManagerCompat.from(context).areNotificationsEnabled()
        val record = call(context,"claim",JSONObject().put("key",key).put("permission",permitted)) as? JSONObject ?: return
        val plan = record.getJSONObject("plan")
        try {
            val sound = plan.getBoolean("sound"); val vibration = plan.getBoolean("vibration")
            val channelId = "local-${sound}-${vibration}"
            val manager = context.getSystemService(NotificationManager::class.java)
            val channel = NotificationChannel(channelId, context.getString(R.string.notification_channel), NotificationManager.IMPORTANCE_DEFAULT)
            channel.enableVibration(vibration)
            if (!sound) channel.setSound(null,null)
            manager.createNotificationChannel(channel)
            val open = PendingIntent.getActivity(context,0,Intent(context,MainActivity::class.java).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val notification = NotificationCompat.Builder(context,channelId).setSmallIcon(R.drawable.ic_notification).setContentTitle(plan.getString("title")).setContentText(plan.getString("body")).setStyle(NotificationCompat.BigTextStyle().bigText(plan.getString("body"))).setAutoCancel(true).setContentIntent(open).build()
            manager.notify(record.getLong("id").toInt(),notification)
        } catch (error: Exception) {
            call(context,"ack",JSONObject().put("key",key)); Log.e("PowerWatch","OS notification rejected",error)
        }
    }
}

class MonitorWorker(context: Context, params: WorkerParameters): Worker(context,params) {
    override fun doWork(): Result = try {
        val next = BackgroundBridge.run(applicationContext)
        BackgroundBridge.enqueue(applicationContext,next,ExistingWorkPolicy.APPEND_OR_REPLACE)
        Result.success()
    } catch (error: Exception) { Log.e("PowerWatch","Local work failed",error); Result.retry() }
}
class PulseWorker(context: Context, params: WorkerParameters): Worker(context,params) {
    override fun doWork(): Result = try { BackgroundBridge.syncAlarms(applicationContext);PowerWatchWidget.updateAll(applicationContext);BackgroundBridge.enqueue(applicationContext,0,ExistingWorkPolicy.KEEP);Result.success() }catch(error:Exception){Log.e("PowerWatch","Local widget pulse failed",error);Result.retry()}
}
class ReminderReceiver: BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        Executors.newSingleThreadExecutor().apply { execute { try { intent.getStringExtra("key")?.let { BackgroundBridge.deliver(context,it) } } catch (error: Exception) {Log.e("PowerWatch","Reminder failed",error)} finally {pending.finish();shutdown()} } }
    }
}
class BootReceiver: BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if(intent.action==Intent.ACTION_MY_PACKAGE_REPLACED)PowerWatchWidget.refreshDefinitions(context)
        if (intent.action in setOf(Intent.ACTION_BOOT_COMPLETED,Intent.ACTION_MY_PACKAGE_REPLACED,Intent.ACTION_TIME_CHANGED,Intent.ACTION_TIMEZONE_CHANGED)) {
            BackgroundBridge.enqueueLocal(context,ExistingWorkPolicy.REPLACE)
        }
    }
}

class RefreshWorker(context: Context, params: WorkerParameters): Worker(context,params) {
    override fun doWork(): Result = try {
        BackgroundBridge.run(applicationContext)
        if(BackgroundBridge.call(applicationContext,"manual-pending")==true) Result.retry() else Result.success()
    } catch(error:Exception){ Log.e("PowerWatch","Widget refresh failed",error);Result.retry() }
}
