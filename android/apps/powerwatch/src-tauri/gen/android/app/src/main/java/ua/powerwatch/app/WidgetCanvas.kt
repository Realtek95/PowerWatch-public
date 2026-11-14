package ua.powerwatch.app

import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.text.StaticLayout
import android.text.Layout
import android.text.TextPaint
import android.text.TextUtils
import org.json.JSONObject
import kotlin.math.cos
import kotlin.math.sin

/** Paints Rust's cached projection. Calendar boundaries and fractions stay in Rust. */
object WidgetCanvas {
    fun draw(context: Context, value: JSONObject?, width: Int, height: Int, kind: String): Bitmap {
        val scale = context.resources.displayMetrics.density.coerceAtMost(2f)
        val bitmap = Bitmap.createBitmap(
            (width * scale).toInt().coerceAtLeast(1),
            (height * scale).toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888
        )
        bitmap.density=(scale*160).toInt()
        val canvas = Canvas(bitmap)
        canvas.scale(scale, scale)
        Painter(context, value, canvas, width.toFloat(), height.toFloat()).draw(kind)
        return bitmap
    }

    private class Painter(
        val context: Context, val value: JSONObject?, val canvas: Canvas,
        val width: Float, val height: Float
    ) {
        private val theme = value?.optString("theme") ?: "system"
        private val dark = theme == "dark" || (theme == "system" &&
            context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES)
        private val background = color("#f8fafc", "#1c292f")
        private val surface = color("#ffffff", "#233137")
        private val ink = color("#19232c", "#f1f5f6")
        private val muted = color("#657382", "#b5c3ca")
        private val green = color("#187e25", "#8add52")
        private val line = color("#e4e9ee", "#405057")
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private val fontScale = context.resources.configuration.fontScale.coerceIn(1f, 1.3f)
        private val stale = value?.optBoolean("stale", true) ?: true
        private val state = if (stale) "unknown" else value?.optString("state") ?: "unknown"
        private val label = context.getString(when (state) {
            "available" -> R.string.widget_available
            "outage" -> R.string.widget_outage
            "switching" -> R.string.widget_switching
            else -> if (stale && value?.isNull("ageSeconds") == false) R.string.widget_stale else R.string.widget_no_data
        })
        private val tint = when (state) {
            "available" -> color("#e0f8df", "#194c2c")
            "outage" -> color("#ffebe6", "#553532")
            else -> surface
        }
        private val refreshing = value?.optBoolean("refreshing",false) == true
        private val queue = value?.optString("queue")?.ifEmpty { "—" } ?: "—"
        private val city = value?.optString("city")?.ifEmpty {
            context.getString(R.string.widget_choose_address)
        } ?: context.getString(R.string.widget_choose_address)
        private val event = value?.optString("eventDetail") ?: context.getString(R.string.widget_no_event)
        private val eventCaption = value?.optString("eventCaption") ?: ""
        private val projectionTime = context.getString(R.string.widget_projection_time, value?.optString("displayTime") ?: "—")

        private fun color(light: String, dark: String) = Color.parseColor(if (this.dark) dark else light)

        fun draw(kind: String) {
            val compact = kind == "small"
            if(kind=="chart") {
                box(0f,0f,width,height,background,22f)
                text(if(refreshing)context.getString(R.string.widget_refreshing) else value?.optString("checked")?:"",9f,20f,7f,muted,max=width-84f)
                refresh(width-56f,19f);box(width-36f,5f,27f,28f,surface,9f);gear(width-22f,19f)
                chart(6f,width-12f,34f)
                return
            }
            box(0f, 0f, width, height, background, 22f)
            val headerCompact = compact || kind == "medium"
            val tiny = compact && height < 60f
            bolt(9f, if(tiny)4f else 6f, if(tiny)16f else if(headerCompact)20f else 25f)
            text("PowerWatch", if(headerCompact)35f else 43f, if(tiny)16f else if(headerCompact)20f else 28f, if(tiny)9f else if(headerCompact)11f else 15f, bold = true, max = width - if (compact) 60f else 106f)
            refresh(width - if (compact) 18f else 56f, if(tiny)14f else if(headerCompact)19f else 23f)
            if (refreshing && kind == "medium") text(context.getString(R.string.widget_refreshing),35f,30f,6f,green,max=width-78f)
            if (!compact) {
                box(width - 36f, 8f, 27f, 28f, surface, 9f)
                gear(width - 22f, 22f)
            }
            when {
                compact -> small()
                kind == "large" && width >= 300f && height >= 180f -> large()
                else -> medium()
            }
        }

        private fun small() {
            if(height>=60f)text(if(refreshing)context.getString(R.string.widget_refreshing) else projectionTime,35f,28f,6f,if(refreshing)green else muted,max=width-55f)
            val top = if(height<60f)24f else 33f
            val panelHeight = height - top - 5f
            box(6f,top,width-12f,panelHeight,tint,12f)
            val diameter = if(panelHeight>=24f)22f else 14f
            bolt(11f,top+(panelHeight-diameter)/2f,diameter,round=true)
            val shortLabel = context.getString(when(state){
                "available"->R.string.widget_available_short
                "outage"->R.string.widget_outage_short
                else->if(stale && value?.isNull("ageSeconds")==false)R.string.widget_stale_short else R.string.widget_unknown_short
            })
            if(panelHeight>=24f) {
                fun metrics(size:Float)=TextPaint().apply{textSize=size*fontScale}.fontMetrics
                val natural=metrics(11f).let{it.descent-it.ascent}+metrics(8f).let{it.descent-it.ascent}+2f
                val fit=((panelHeight-4f)/natural).coerceAtMost(1f)
                val titleMetrics=metrics(11f*fit);val queueMetrics=metrics(8f*fit)
                val groupHeight=titleMetrics.descent-titleMetrics.ascent+queueMetrics.descent-queueMetrics.ascent+2f
                val titleY=top+(panelHeight-groupHeight)/2f-titleMetrics.ascent
                text(shortLabel,39f,titleY,11f*fit,bold=true,max=width-47f)
                text(context.getString(R.string.widget_queue,queue),39f,titleY+titleMetrics.descent+2f-queueMetrics.ascent,8f*fit,muted,max=width-47f)
            } else {
                text(if(refreshing)context.getString(R.string.widget_refreshing) else shortLabel,29f,top+panelHeight/2f+3f,8f,bold=true,max=width-58f)
                text(queue,width-26f,top+panelHeight/2f+3f,8f,muted,max=20f)
            }
        }

        private fun medium() {
            val short = height < 145f
            val cityY = if(short)33f else 40f
            val cityH = if(short)30f else 40f
            box(8f,cityY,width-16f,cityH,surface,10f)
            pin(20f,cityY+12f,6f)
            text(city,33f,cityY+13f,if(short)10f else 12f,bold=true,max=width-45f)
            text(context.getString(R.string.widget_queue,queue),33f,cityY+cityH-5f,if(short)8f else 10f,green,true,width-45f)
            val statusY=cityY+cityH+6f
            val statusH=if(short)22f else 30f
            box(8f,statusY,width-16f,statusH,tint,10f)
            bolt(13f,statusY+3f,statusH-6f,round=true)
            text(label,statusH+13f,statusY+statusH/2f+4f,if(short)10f else 11f,bold=true,max=width-statusH-24f)
            val baseline=height-8f
            calendar(20f,baseline-7f,6f)
            if(!short)text(eventCaption,33f,baseline-12f,8f,muted,max=width-43f)
            text(event,33f,baseline,if(short)8f else 9f,bold=true,max=width-43f)
        }

        private fun large() {
            val left = (width * .40f).coerceAtLeast(125f)
            val chartX = left + 17f
            val chartW = width - chartX - 10f
            text(if(refreshing)context.getString(R.string.widget_refreshing) else value?.optString("checked") ?: "", width * .48f, 27f, 8f, muted, max = width * .29f)
            box(10f, 43f, left - 10f, 38f, surface)
            pin(23f, 61f, 8f)
            text(city, 38f, 60f, 12f, bold = true, max = left - 44f)
            text(value?.optString("street") ?: "", 38f, 75f, 8f, muted, max = left - 44f)
            box(10f, 87f, left - 10f, 24f, surface)
            val operator = value?.optString("operator") ?: ""
            text(context.getString(R.string.widget_queue, queue), 18f, 103f, 12f, green, true,
                max = left - if (operator.isEmpty()) 26f else 62f)
            if (operator.isNotEmpty()) {
                box(left - 42f, 91f, 34f, 17f, line, 8f)
                text(if (operator.contains("ДТЕК")) "ДТЕК" else operator, left - 39f, 103f, 8f, muted, max = 28f)
            }
            box(10f, 117f, left - 10f, height - 127f, tint)
            bolt(17f, 125f, 26f, round = true)
            val words = label.split(" ")
            val split = if (words.size > 2) words.size - 2 else 1
            text(words.take(split).joinToString(" "), 49f, 132f, 10f, bold = true, max = left - 55f)
            text(words.drop(split).joinToString(" "), 49f, 145f, 10f, bold = true, max = left - 55f)
            paragraph(listOf(eventCaption,event).filter{it.isNotEmpty()}.joinToString("\n"),18f,154f,left-26f,height-169f,if(height<195f)6f else 8f)
            chart(chartX, chartW)
        }

        private fun chart(chartX: Float, chartW: Float, panelTop:Float=45f) {
            val vertical=((height-panelTop-40f)/100f).coerceIn(.5f,1f)
            fun y(offset:Float)=panelTop+offset*vertical
            box(chartX, panelTop, chartW, height - panelTop - 10f, surface)
            calendar(chartX + 13f, y(13f), 6f)
            text(context.getString(R.string.widget_schedule), chartX + 24f, y(16f), 10f, bold = true, max = chartW - 33f)
            text(value?.optString("dayLabel") ?: "", chartX + 9f, y(31f), 8f, green, true, chartW - 18f)
            val bands = value?.optJSONArray("bands")
            val columnW = (chartW - 18f) / 2f
            for (column in 0..1) {
                val x = chartX + 8f + column * columnW
                val barX = x + 25f
                val barW = columnW - 30f
                box(x + 1f, y(38f), columnW - 5f, 13f*vertical, line, 4f)
                text(if (column == 0) "00:00 – 12:00" else "12:00 – 24:00", x + 4f, y(47f), 7f, muted, max = columnW - 10f)
                val top = y(59f)
                val bandHeight = (height - top - 20f).coerceAtLeast(14f)
                box(barX, top, barW, bandHeight, color("#cbd1d9", "#5e6a73"), 4f)
                val band = bands?.optJSONObject(column)
                val segments = band?.optJSONArray("segments")
                if (segments != null) for (i in 0 until segments.length()) {
                    val segment = segments.getJSONObject(i)
                    val fill = when (segment.optString("state")) {
                        "available" -> color("#a8efa0", "#469745")
                        "outage" -> color("#aeb8c4", "#707e8b")
                        "switching" -> color("#dfd59d", "#a3985c")
                        else -> color("#e1e5e9", "#48555c")
                    }
                    box(barX, top + bandHeight * segment.optDouble("top").toFloat(), barW,
                        (bandHeight * segment.optDouble("height").toFloat() - 1.5f).coerceAtLeast(.5f), fill, 3f)
                }
                val ticks = band?.optJSONArray("ticks")
                if (ticks != null) for (i in 0 until ticks.length()) {
                    val tick = ticks.getJSONObject(i)
                    val position = tick.optDouble("position").toFloat()
                    val y = top + bandHeight * position
                    if (position > 0f && position < 1f) {
                        paint.color = surface
                        paint.strokeWidth = 1.5f
                        canvas.drawLine(barX, y, barX + barW, y, paint)
                    }
                    if(bandHeight<60f&&i%2==1&&i!=ticks.length()-1)continue
                    val parts = tick.optString("label").split(" ")
                    text(parts[0], x, y + if (parts.size > 1) -1f else 3f, 7f, muted, max = 24f)
                    if (parts.size > 1) text(parts[1], x, y + 6f, 5f, muted, max = 24f)
                }
                if (!stale && band?.isNull("nowPosition") == false) {
                    val y = top + bandHeight * band.optDouble("nowPosition").toFloat()
                    paint.color = green
                    paint.strokeWidth = 1.2f
                    canvas.drawLine(barX, y, barX + barW, y, paint)
                    val badgeY = (y - 15f).coerceIn(top, top + bandHeight - 14f)
                    box(barX + barW - 31f, badgeY, 31f, 14f, green, 4f)
                    text(value?.optString("displayTime") ?: "", barX + barW - 28f, badgeY + 10f, 7f, Color.WHITE, max = 29f)
                }
            }
        }

        private fun paragraph(value:String,x:Float,top:Float,w:Float,h:Float,size:Float) {
            if(h<=0f||w<=0f)return
            var fontSize=size
            val font=TextPaint(Paint.ANTI_ALIAS_FLAG).apply{color=ink;textSize=fontSize*fontScale;typeface=Typeface.DEFAULT_BOLD}
            fun fullLayout()=StaticLayout.Builder.obtain(value,0,value.length,font,w.toInt()).setAlignment(Layout.Alignment.ALIGN_NORMAL).setIncludePad(false).setLineSpacing(2f,1f).build()
            var layout=fullLayout()
            while(layout.height>h&&fontSize>6f){fontSize-=.5f;font.textSize=fontSize*fontScale;layout=fullLayout()}
            if(layout.height>h) {
                val lineHeight=font.fontMetrics.let{it.descent-it.ascent}+2f
                layout=StaticLayout.Builder.obtain(value,0,value.length,font,w.toInt()).setIncludePad(false).setLineSpacing(2f,1f).setMaxLines((h/lineHeight).toInt().coerceAtLeast(1)).setEllipsize(TextUtils.TruncateAt.END).build()
            }
            canvas.save();canvas.translate(x,top);layout.draw(canvas);canvas.restore()
        }

        private fun box(x: Float, y: Float, w: Float, h: Float, color: Int, r: Float = 12f) {
            paint.style = Paint.Style.FILL
            paint.color = color
            canvas.drawRoundRect(x, y, x + w, y + h, r, r, paint)
        }

        private fun text(value: String, x: Float, y: Float, size: Float, color: Int = ink,
                         bold: Boolean = false, max: Float = width - x - 10f) {
            val font = TextPaint(Paint.ANTI_ALIAS_FLAG)
            font.color = color
            font.textSize = size * fontScale
            font.typeface = if (bold) Typeface.DEFAULT_BOLD else Typeface.DEFAULT
            canvas.drawText(TextUtils.ellipsize(value, font, max.coerceAtLeast(1f), TextUtils.TruncateAt.END).toString(), x, y, font)
        }

        private fun bolt(x: Float, y: Float, size: Float, round: Boolean = false) {
            box(x, y, size, size, Color.rgb(38, 183, 43), size * if (round) .5f else .27f)
            context.getDrawable(R.drawable.ic_notification)?.apply {
                setBounds((x + size * .18f).toInt(), (y + size * .15f).toInt(), (x + size * .82f).toInt(), (y + size * .85f).toInt())
                setTint(Color.WHITE)
                draw(canvas)
            }
        }

        private fun stroke(color: Int = green, width: Float = 1.5f) {
            paint.style = Paint.Style.STROKE
            paint.color = color
            paint.strokeWidth = width
        }

        private fun pin(cx: Float, cy: Float, size: Float) {
            stroke()
            val path = Path()
            path.moveTo(cx, cy + size)
            path.cubicTo(cx - size * 1.7f, cy - size * .6f, cx - size * .5f, cy - size * 1.5f, cx, cy - size)
            path.cubicTo(cx + size * .5f, cy - size * 1.5f, cx + size * 1.7f, cy - size * .6f, cx, cy + size)
            canvas.drawPath(path, paint)
            canvas.drawCircle(cx, cy - size * .3f, size * .25f, paint)
            paint.style = Paint.Style.FILL
        }

        private fun calendar(cx: Float, cy: Float, size: Float) {
            stroke()
            canvas.drawRoundRect(cx - size, cy - size * .7f, cx + size, cy + size, 2f, 2f, paint)
            canvas.drawLine(cx - size, cy - size * .15f, cx + size, cy - size * .15f, paint)
            for (offset in listOf(-.5f, .5f)) canvas.drawLine(cx + size * offset, cy - size, cx + size * offset, cy - size * .4f, paint)
            paint.style = Paint.Style.FILL
        }

        private fun refresh(cx: Float, cy: Float) {
            stroke(muted, 1.7f)
            canvas.drawArc(RectF(cx - 6f, cy - 6f, cx + 6f, cy + 6f), 35f, 285f, false, paint)
            canvas.drawLine(cx + 6f, cy - 6f, cx + 6f, cy, paint)
            canvas.drawLine(cx + 6f, cy, cx + 1f, cy, paint)
            paint.style = Paint.Style.FILL
        }

        private fun gear(cx: Float, cy: Float) {
            stroke(muted)
            canvas.drawCircle(cx, cy, 5f, paint)
            canvas.drawCircle(cx, cy, 2f, paint)
            for (i in 0..7) {
                val a = i * Math.PI / 4
                canvas.drawLine(cx + cos(a).toFloat() * 5f, cy + sin(a).toFloat() * 5f,
                    cx + cos(a).toFloat() * 7f, cy + sin(a).toFloat() * 7f, paint)
            }
            paint.style = Paint.Style.FILL
        }
    }
}
