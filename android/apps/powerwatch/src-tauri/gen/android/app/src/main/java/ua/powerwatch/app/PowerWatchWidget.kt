package ua.powerwatch.app

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.View
import android.widget.RemoteViews
import androidx.work.*
import org.json.JSONObject
import java.util.concurrent.Executors
import android.util.Log
import android.util.SizeF
import android.os.Build
import androidx.core.os.BundleCompat

open class PowerWatchWidget: AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) { refresh(context) }
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) { refresh(context) }
    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context,intent)
        if(intent.action==REFRESH) {
            val pending=goAsync()
            Executors.newSingleThreadExecutor().apply { execute {
                try {
                    BackgroundBridge.call(context,"request-refresh")
                    updateAll(context,true)
                    val request=OneTimeWorkRequestBuilder<RefreshWorker>().setBackoffCriteria(BackoffPolicy.LINEAR,10,java.util.concurrent.TimeUnit.SECONDS).build()
                    WorkManager.getInstance(context).enqueueUniqueWork("powerwatch-manual-refresh",ExistingWorkPolicy.KEEP,request)
                } catch(error:Exception){Log.e("PowerWatch","Widget refresh request failed",error)}finally{pending.finish();shutdown()}
            } }
        }
    }
    private fun refresh(context: Context) {
        val pending=goAsync()
        Executors.newSingleThreadExecutor().apply { execute {try {updateAll(context,true)}catch(error:Exception){Log.e("PowerWatch","Widget update failed",error)}finally{pending.finish();shutdown()}} }
    }
    companion object {
        private const val REFRESH="ua.powerwatch.app.WIDGET_REFRESH"
        private val lastProjection=mutableMapOf<Int,String>()
        /** Refresh Android's persisted provider definition, including OEM picker caches. */
        fun refreshDefinitions(context:Context) {
            val preferences=context.getSharedPreferences("powerwatch",0)
            val code=context.packageManager.getPackageInfo(context.packageName,0).longVersionCode
            if(preferences.getLong("widgetDefinitionCode",-1)==code)return
            val manager=AppWidgetManager.getInstance(context)
            var complete=true
            for(provider in listOf(PowerWatchWidget::class.java,PowerWatchMediumWidget::class.java,PowerWatchSmallWidget::class.java,PowerWatchChartWidget::class.java)) {
                try {
                    val component=ComponentName(context,provider)
                    manager.updateAppWidgetProviderInfo(component,null)
                    manager.updateAppWidgetProviderInfo(component,AppWidgetManager.META_DATA_APPWIDGET_PROVIDER)
                }catch(error:RuntimeException){complete=false;Log.w("PowerWatch","Widget definition refresh failed",error)}
            }
            if(complete)preferences.edit().putLong("widgetDefinitionCode",code).apply()
        }
        fun renderSizes(context:Context,value:JSONObject?,options:Bundle,kind:String):RemoteViews {
            if(Build.VERSION.SDK_INT>=31) {
                val sizes=BundleCompat.getParcelableArrayList(options,AppWidgetManager.OPTION_APPWIDGET_SIZES,SizeF::class.java)
                    ?.filter{it.width.isFinite()&&it.height.isFinite()&&it.width>=40&&it.height>=24&&it.width<=1000&&it.height<=700}?.distinct()?.take(8)
                if(!sizes.isNullOrEmpty())return RemoteViews(sizes.associateWith{render(context,value,it.width.toInt(),it.height.toInt(),kind)})
            }
            val width=options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH,when(kind){"small","chart"->180;"medium"->250;else->360}).coerceIn(80,700)
            val height=options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT,when(kind){"small"->64;"chart"->180;"medium"->150;else->210}).coerceIn(40,400)
            val landscapeWidth=options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH,width).coerceIn(80,1000)
            val landscapeHeight=options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT,height).coerceIn(40,400)
            return RemoteViews(render(context,value,landscapeWidth,landscapeHeight,kind),render(context,value,width,height,kind))
        }
        @Synchronized fun updateAll(context: Context, force: Boolean = false) {
            val manager=AppWidgetManager.getInstance(context)
            val providers=listOf(PowerWatchWidget::class.java,PowerWatchMediumWidget::class.java,PowerWatchSmallWidget::class.java,PowerWatchChartWidget::class.java)
            val ids=providers.flatMap{manager.getAppWidgetIds(ComponentName(context,it)).toList()}
            if(ids.isEmpty())return
            val value=BackgroundBridge.call(context,"widget") as? JSONObject
            val stable=value?.let{JSONObject(it.toString())}
            stable?.remove("ageSeconds")
            val bands=stable?.optJSONArray("bands")
            if(bands!=null)for(i in 0 until bands.length())bands.getJSONObject(i).remove("nowPosition")
            val fingerprint=stable?.toString()?:"empty"
            for(id in ids){
                if(!force&&lastProjection[id]==fingerprint)continue
                val options=manager.getAppWidgetOptions(id)
                val provider=manager.getAppWidgetInfo(id)?.provider?.className?:""
                val kind=when{provider.endsWith("ChartWidget")->"chart";provider.endsWith("SmallWidget")->"small";provider.endsWith("MediumWidget")->"medium";else->"large"}
                manager.updateAppWidget(id,renderSizes(context,value,options,kind))
                lastProjection[id]=fingerprint
            }
        }
        fun render(context: Context,value: JSONObject?,width:Int=360,height:Int=210,kind:String="large"): RemoteViews {
            val views=RemoteViews(context.packageName,R.layout.powerwatch_widget)
            views.setImageViewBitmap(R.id.widget_image,WidgetCanvas.draw(context,value,width,if(kind=="small")height.coerceAtMost(80) else height,kind))
            views.setContentDescription(R.id.widget_root,listOfNotNull(value?.optString("address"),value?.optString("city"),value?.optString("street"),value?.optString("queue"),value?.optString("forecast"),value?.optString("source"),value?.optString("event"),value?.optString("checked")).joinToString(". "))
            val open=Intent(context,MainActivity::class.java).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP).putExtra("powerwatchHome",true)
            views.setOnClickPendingIntent(R.id.widget_root,PendingIntent.getActivity(context,100,open,PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            val settings=Intent(context,MainActivity::class.java).setFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP).putExtra("powerwatchSettings",true)
            views.setOnClickPendingIntent(R.id.widget_settings,PendingIntent.getActivity(context,101,settings,PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            views.setViewVisibility(R.id.widget_settings,if(kind=="small")View.GONE else View.VISIBLE)
            val refresh=Intent(context,PowerWatchWidget::class.java).setAction(REFRESH)
            val busy = value?.optBoolean("refreshing",false) == true
            views.setContentDescription(R.id.widget_refresh,context.getString(if(busy)R.string.widget_refreshing else R.string.widget_refresh))
            views.setOnClickPendingIntent(R.id.widget_refresh,if(busy)null else PendingIntent.getBroadcast(context,102,refresh,PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            return views
        }
    }
}
class PowerWatchMediumWidget: PowerWatchWidget()
class PowerWatchSmallWidget: PowerWatchWidget()

class PowerWatchChartWidget: PowerWatchWidget()
