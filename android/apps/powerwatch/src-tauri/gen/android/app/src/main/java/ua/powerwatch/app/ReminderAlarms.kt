package ua.powerwatch.app

import android.app.ActivityManager
import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import org.json.JSONObject
import java.time.Instant

/** One wake-up for the earliest durable Rust deadline; never polls the network. */
object ReminderAlarms {
    private fun preferences(context: Context) = context.getSharedPreferences("powerwatch", 0)
    private fun intent(context: Context) = Intent(context, ReminderReceiver::class.java)
        .setData(Uri.parse("powerwatch://reminders"))
    private fun pending(context: Context) = PendingIntent.getBroadcast(context, 0, intent(context), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    fun exactAllowed(context: Context): Boolean = Build.VERSION.SDK_INT < 31 || context.getSystemService(AlarmManager::class.java).canScheduleExactAlarms()
    fun status(context: Context): JSONObject {
        val power = context.getSystemService(PowerManager::class.java)
        return JSONObject().put("exactAllowed", exactAllowed(context))
            .put("exactAccessRequired", Build.VERSION.SDK_INT >= 31)
            .put("batterySaver", power.isPowerSaveMode)
            .put("backgroundRestricted", context.getSystemService(ActivityManager::class.java).isBackgroundRestricted)
    }
    @Synchronized fun invalidate(context: Context) {
        preferences(context).edit().remove("reminderDeadline").apply()
    }
    @Synchronized fun cancel(context: Context) {
        context.getSystemService(AlarmManager::class.java).cancel(pending(context))
        preferences(context).edit().remove("reminderDeadline").remove("reminderTriggerAt").apply()
    }
    @Synchronized fun received(context: Context) {
        val prefs = preferences(context)
        val now = System.currentTimeMillis()
        prefs.edit().remove("reminderDeadline").putLong("reminderReceivedAt", now)
            .putLong("reminderLatenessMs", (now - prefs.getLong("reminderTriggerAt", now)).coerceAtLeast(0)).apply()
    }
    @Synchronized fun sync(context: Context) {
        val value = BackgroundBridge.call(context, "pending") as? JSONObject ?: return
        val prefs = preferences(context)
        val manager = context.getSystemService(AlarmManager::class.java)
        // Cancel registrations left by the previous per-record OS adapter on update.
        for (key in prefs.getStringSet("alarms", emptySet())!!) {
            val old = Intent(context, ReminderReceiver::class.java).setData(Uri.parse("powerwatch://reminder/${Uri.encode(key)}"))
            PendingIntent.getBroadcast(context, 0, old, PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE)?.let {
                manager.cancel(it); it.cancel()
            }
        }
        prefs.edit().remove("alarms").apply()
        val now = Instant.parse(value.getString("now")).toEpochMilli()
        val records = value.getJSONArray("notifications")
        var next: Long? = null
        for (index in 0 until records.length()) {
            val plan = records.getJSONObject(index).getJSONObject("plan")
            val at = Instant.parse(plan.getString("at")).toEpochMilli()
            if (at <= now) BackgroundBridge.deliver(context, plan.getString("key"))
            else if (next == null || at < next) next = at
        }
        if (next == null) {
            cancel(context)
            return
        }
        var exact = exactAllowed(context)
        val deadline = "${prefs.getString("database", "")}:$next:$exact"
        if (prefs.getString("reminderDeadline", null) == deadline) return
        // The private fixture clock also uses this delta; production moments are UTC.
        val trigger = System.currentTimeMillis() + (next - now).coerceAtLeast(100)
        val alarm = pending(context)
        if (exact) {
            try { manager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, trigger, alarm) }
            catch (_: SecurityException) { exact = false; manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, trigger, alarm) }
        } else manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, trigger, alarm)
        prefs.edit().putString("reminderDeadline", "${prefs.getString("database", "")}:$next:$exact")
            .putBoolean("reminderExact", exact).putLong("reminderTriggerAt", trigger)
            .putLong("reminderRegistrations", prefs.getLong("reminderRegistrations", 0) + 1).apply()
    }
}
