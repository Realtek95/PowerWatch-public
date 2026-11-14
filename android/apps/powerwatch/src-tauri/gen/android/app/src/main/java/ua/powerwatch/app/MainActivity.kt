package ua.powerwatch.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  private var permissionFlowReady = false
  var notificationPermissionPending = false
    private set
  var notificationPermissionChanged: (() -> Unit)? = null
  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode == 501) {
      notificationPermissionPending = false
      notificationPermissionChanged?.invoke()
      window.decorView.post { requestExactRemindersOnce() }
    }
  }
  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) notificationPermissionChanged?.invoke()
  }
  override fun onResume() {
    super.onResume()
    BackgroundBridge.startForeground(this)
    if (permissionFlowReady) window.decorView.post { requestExactRemindersOnce() }
  }
  private fun requestExactRemindersOnce() {
    if (android.os.Build.VERSION.SDK_INT < 31 || notificationPermissionPending || isFinishing) return
    val preferences = getSharedPreferences("powerwatch", 0)
    if (preferences.getBoolean("exactReminderPermissionRequested", false)) return
    if (!preferences.edit().putBoolean("exactReminderPermissionRequested", true).commit()) return
    if (!ReminderAlarms.exactAllowed(this)) {
      try {
        startActivity(android.content.Intent(android.provider.Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, android.net.Uri.parse("package:$packageName")))
      } catch (_: android.content.ActivityNotFoundException) {
        // The app settings retain a retry button if the system page is absent.
      }
    }
  }
  override fun onPause() { BackgroundBridge.stopForeground(); super.onPause() }
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val content = findViewById<android.view.View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(content)
    content.post {
      permissionFlowReady = true
      val preferences = getSharedPreferences("powerwatch", 0)
      if (android.os.Build.VERSION.SDK_INT >= 33 && !preferences.getBoolean("notificationPermissionRequested", false)) {
        preferences.edit().putBoolean("notificationPermissionRequested", true).commit()
        if (androidx.core.content.ContextCompat.checkSelfPermission(this, android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
          notificationPermissionPending = true
          androidx.core.app.ActivityCompat.requestPermissions(this, arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 501)
        }
      }
      requestExactRemindersOnce()
    }
  }
}
