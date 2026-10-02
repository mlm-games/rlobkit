package rust.rlobkit

import android.content.res.Configuration
import android.content.res.Resources
import android.os.Build
import kotlin.math.pow
import kotlin.math.roundToInt

/**
 * The Material 3 roles of the system's wallpaper-derived theme, packed for
 * `nativeOnTheme`: [COUNT] RGBA quads in `repose_core::locals::ColorScheme`
 * field order.
 */
internal object DynamicTheme {

    private const val COUNT = 38
    private const val ACCENT1 = "accent1"
    private const val ACCENT2 = "accent2"
    private const val ACCENT3 = "accent3"
    private const val NEUTRAL2 = "neutral2"

    private val ACCENT_TONES = intArrayOf(10, 20, 30, 40, 80, 90, 100)
    private val NEUTRAL_TONES = intArrayOf(0, 10, 20, 30, 50, 60, 80, 90, 95, 99, 100)

    fun isNight(config: Configuration): Boolean =
        (config.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
            Configuration.UI_MODE_NIGHT_YES

    /** The packed palette, or null on systems older than Android 12. */
    fun build(res: Resources, dark: Boolean): ByteArray? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            return null
        }
        val roles = IntArray(COUNT)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            fromRoleResources(res, dark, roles)
        } else {
            fromTonalPalettes(res, dark, roles)
        }
        return pack(roles)
    }

    /** Android 14 and later publish every scheme role as a colour resource. */
    private fun fromRoleResources(res: Resources, dark: Boolean, out: IntArray) {
        fun pick(light: Int, darkColor: Int) = res.getColor(if (dark) darkColor else light, null)

        out[0] = pick(android.R.color.system_primary_light, android.R.color.system_primary_dark)
        out[1] = pick(android.R.color.system_on_primary_light, android.R.color.system_on_primary_dark)
        out[2] =
            pick(
                android.R.color.system_primary_container_light,
                android.R.color.system_primary_container_dark,
            )
        out[3] =
            pick(
                android.R.color.system_on_primary_container_light,
                android.R.color.system_on_primary_container_dark,
            )
        out[4] = pick(android.R.color.system_secondary_light, android.R.color.system_secondary_dark)
        out[5] =
            pick(
                android.R.color.system_on_secondary_light,
                android.R.color.system_on_secondary_dark,
            )
        out[6] =
            pick(
                android.R.color.system_secondary_container_light,
                android.R.color.system_secondary_container_dark,
            )
        out[7] =
            pick(
                android.R.color.system_on_secondary_container_light,
                android.R.color.system_on_secondary_container_dark,
            )
        out[8] = pick(android.R.color.system_tertiary_light, android.R.color.system_tertiary_dark)
        out[9] =
            pick(android.R.color.system_on_tertiary_light, android.R.color.system_on_tertiary_dark)
        out[10] =
            pick(
                android.R.color.system_tertiary_container_light,
                android.R.color.system_tertiary_container_dark,
            )
        out[11] =
            pick(
                android.R.color.system_on_tertiary_container_light,
                android.R.color.system_on_tertiary_container_dark,
            )
        out[12] = pick(android.R.color.system_error_light, android.R.color.system_error_dark)
        out[13] = pick(android.R.color.system_on_error_light, android.R.color.system_on_error_dark)
        out[14] =
            pick(
                android.R.color.system_error_container_light,
                android.R.color.system_error_container_dark,
            )
        out[15] =
            pick(
                android.R.color.system_on_error_container_light,
                android.R.color.system_on_error_container_dark,
            )
        out[16] = pick(android.R.color.system_background_light, android.R.color.system_background_dark)
        out[17] =
            pick(android.R.color.system_on_background_light, android.R.color.system_on_background_dark)
        out[18] = pick(android.R.color.system_surface_light, android.R.color.system_surface_dark)
        out[19] = pick(android.R.color.system_on_surface_light, android.R.color.system_on_surface_dark)
        out[20] =
            pick(
                android.R.color.system_surface_variant_light,
                android.R.color.system_surface_variant_dark,
            )
        out[21] =
            pick(
                android.R.color.system_on_surface_variant_light,
                android.R.color.system_on_surface_variant_dark,
            )
        out[22] =
            pick(
                android.R.color.system_surface_container_lowest_light,
                android.R.color.system_surface_container_lowest_dark,
            )
        out[23] =
            pick(
                android.R.color.system_surface_container_low_light,
                android.R.color.system_surface_container_low_dark,
            )
        out[24] =
            pick(
                android.R.color.system_surface_container_light,
                android.R.color.system_surface_container_dark,
            )
        out[25] =
            pick(
                android.R.color.system_surface_container_high_light,
                android.R.color.system_surface_container_high_dark,
            )
        out[26] =
            pick(
                android.R.color.system_surface_container_highest_light,
                android.R.color.system_surface_container_highest_dark,
            )
        out[27] =
            pick(android.R.color.system_surface_bright_light, android.R.color.system_surface_bright_dark)
        out[28] = pick(android.R.color.system_surface_dim_light, android.R.color.system_surface_dim_dark)
        out[29] = out[0]
        // The inverse roles are the other variant's surface and primary.
        out[30] = pick(android.R.color.system_surface_dark, android.R.color.system_surface_light)
        out[31] = pick(android.R.color.system_on_surface_dark, android.R.color.system_on_surface_light)
        out[32] = pick(android.R.color.system_primary_dark, android.R.color.system_primary_light)
        out[33] = pick(android.R.color.system_outline_light, android.R.color.system_outline_dark)
        out[34] =
            pick(android.R.color.system_outline_variant_light, android.R.color.system_outline_variant_dark)
        out[35] = res.getColor(android.R.color.system_neutral2_1000, null)
        out[36] = out[35]
        out[37] = out[0]
    }

    /**
     * Android 12 and 13 publish only the five tonal palettes, so the roles are
     * the Material 3 tone assignments built from them. Tone stops the palettes
     * lack are interpolated below.
     */
    private fun fromTonalPalettes(res: Resources, dark: Boolean, out: IntArray) {
        val primary = if (dark) 80 else 40
        val onPrimary = if (dark) 20 else 100
        val container = if (dark) 30 else 90
        val onContainer = if (dark) 90 else 10

        out[0] = tone(res, ACCENT1, primary)
        out[1] = tone(res, ACCENT1, onPrimary)
        out[2] = tone(res, ACCENT1, container)
        out[3] = tone(res, ACCENT1, onContainer)
        out[4] = tone(res, ACCENT2, primary)
        out[5] = tone(res, ACCENT2, onPrimary)
        out[6] = tone(res, ACCENT2, container)
        out[7] = tone(res, ACCENT2, onContainer)
        out[8] = tone(res, ACCENT3, primary)
        out[9] = tone(res, ACCENT3, onPrimary)
        out[10] = tone(res, ACCENT3, container)
        out[11] = tone(res, ACCENT3, onContainer)
        // The system exposes the error roles only from Android 14; these are
        // the Material 3 baselines the scheme falls back to.
        out[12] = if (dark) 0xFFF2B8B5.toInt() else 0xFFBA1A1A.toInt()
        out[13] = if (dark) 0xFF601410.toInt() else 0xFFFFFFFF.toInt()
        out[14] = if (dark) 0xFF8C1D18.toInt() else 0xFFFFDAD6.toInt()
        out[15] = if (dark) 0xFFF9DEDC.toInt() else 0xFF410002.toInt()
        out[16] = tone(res, NEUTRAL2, if (dark) 6 else 98)
        out[17] = tone(res, NEUTRAL2, if (dark) 90 else 10)
        out[18] = tone(res, NEUTRAL2, if (dark) 6 else 98)
        out[19] = tone(res, NEUTRAL2, if (dark) 90 else 10)
        out[20] = tone(res, NEUTRAL2, if (dark) 30 else 90)
        out[21] = tone(res, NEUTRAL2, if (dark) 80 else 30)
        out[22] = tone(res, NEUTRAL2, if (dark) 4 else 100)
        out[23] = tone(res, NEUTRAL2, if (dark) 10 else 96)
        out[24] = tone(res, NEUTRAL2, if (dark) 12 else 94)
        out[25] = tone(res, NEUTRAL2, if (dark) 17 else 92)
        out[26] = tone(res, NEUTRAL2, if (dark) 22 else 90)
        out[27] = tone(res, NEUTRAL2, if (dark) 24 else 98)
        out[28] = tone(res, NEUTRAL2, if (dark) 6 else 87)
        out[29] = out[0]
        out[30] = tone(res, NEUTRAL2, if (dark) 90 else 20)
        out[31] = tone(res, NEUTRAL2, if (dark) 20 else 95)
        out[32] = tone(res, ACCENT1, if (dark) 40 else 80)
        out[33] = tone(res, NEUTRAL2, if (dark) 60 else 50)
        out[34] = tone(res, NEUTRAL2, if (dark) 30 else 80)
        out[35] = tone(res, NEUTRAL2, 0)
        out[36] = out[35]
        out[37] = out[0]
    }

    /**
     * A palette colour at [target]. Stops the palettes do not ship (the surface
     * tones) are interpolated between the neighbouring stops in CIE L*a*b*,
     * where the L* axis is exactly the tone scale, so the result lands on the
     * requested tone with the palette's hue.
     */
    private fun tone(res: Resources, palette: String, target: Int): Int {
        val tones = if (palette == NEUTRAL2) NEUTRAL_TONES else ACCENT_TONES
        if (target in tones) {
            return color(res, palette, target)
        }
        var lo = tones.first()
        var hi = tones.last()
        for (stop in tones) {
            if (stop <= target && stop > lo) lo = stop
            if (stop >= target && stop < hi) hi = stop
        }
        val t = (target - lo).toFloat() / (hi - lo)
        return lerpLab(color(res, palette, lo), color(res, palette, hi), t)
    }

    private fun color(res: Resources, palette: String, tone: Int): Int =
        res.getColor(idOf(palette, tone), null)

    private fun idOf(palette: String, tone: Int): Int = when (palette) {
        ACCENT1 -> when (tone) {
            10 -> android.R.color.system_accent1_900
            20 -> android.R.color.system_accent1_800
            30 -> android.R.color.system_accent1_700
            40 -> android.R.color.system_accent1_600
            80 -> android.R.color.system_accent1_200
            90 -> android.R.color.system_accent1_100
            else -> android.R.color.system_accent1_0
        }

        ACCENT2 -> when (tone) {
            10 -> android.R.color.system_accent2_900
            20 -> android.R.color.system_accent2_800
            30 -> android.R.color.system_accent2_700
            40 -> android.R.color.system_accent2_600
            80 -> android.R.color.system_accent2_200
            90 -> android.R.color.system_accent2_100
            else -> android.R.color.system_accent2_0
        }

        ACCENT3 -> when (tone) {
            10 -> android.R.color.system_accent3_900
            20 -> android.R.color.system_accent3_800
            30 -> android.R.color.system_accent3_700
            40 -> android.R.color.system_accent3_600
            80 -> android.R.color.system_accent3_200
            90 -> android.R.color.system_accent3_100
            else -> android.R.color.system_accent3_0
        }

        else -> when (tone) {
            10 -> android.R.color.system_neutral2_900
            20 -> android.R.color.system_neutral2_800
            30 -> android.R.color.system_neutral2_700
            50 -> android.R.color.system_neutral2_500
            60 -> android.R.color.system_neutral2_400
            80 -> android.R.color.system_neutral2_200
            90 -> android.R.color.system_neutral2_100
            95 -> android.R.color.system_neutral2_50
            99 -> android.R.color.system_neutral2_10
            100 -> android.R.color.system_neutral2_0
            else -> android.R.color.system_neutral2_1000
        }
    }

    private fun lerpLab(from: Int, to: Int, t: Float): Int {
        val a = lab(from)
        val b = lab(to)
        return argbFromLab(
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        )
    }

    private fun lab(argb: Int): FloatArray {
        val r = linear(((argb shr 16) and 0xff) / 255f)
        val g = linear(((argb shr 8) and 0xff) / 255f)
        val b = linear((argb and 0xff) / 255f)
        val x = (0.4124f * r + 0.3576f * g + 0.1805f * b) / 0.95047f
        val y = 0.2126f * r + 0.7152f * g + 0.0722f * b
        val z = (0.0193f * r + 0.1192f * g + 0.9505f * b) / 1.08883f
        val fx = pivot(x)
        val fy = pivot(y)
        val fz = pivot(z)
        return floatArrayOf(116f * fy - 16f, 500f * (fx - fy), 200f * (fy - fz))
    }

    private fun argbFromLab(l: Float, a: Float, b: Float): Int {
        val fy = (l + 16f) / 116f
        val fx = fy + a / 500f
        val fz = fy - b / 200f
        val x = 0.95047f * unpivot(fx)
        val y = unpivot(fy)
        val z = 1.08883f * unpivot(fz)
        val r = encode(3.2406f * x - 1.5372f * y - 0.4986f * z)
        val g = encode(-0.9689f * x + 1.8758f * y + 0.0415f * z)
        val bl = encode(0.0557f * x - 0.2040f * y + 1.0570f * z)
        val ri = r.roundToInt().coerceIn(0, 255)
        val gi = g.roundToInt().coerceIn(0, 255)
        val bi = bl.roundToInt().coerceIn(0, 255)
        return (0xff shl 24) or (ri shl 16) or (gi shl 8) or bi
    }

    private fun linear(c: Float): Float =
        if (c <= 0.04045f) c / 12.92f else ((c + 0.055f) / 1.055f).pow(2.4f)

    private fun encode(c: Float): Float =
        if (c <= 0.0031308f) c * 12.92f else 1.055f * c.pow(1f / 2.4f) - 0.055f

    private fun pivot(t: Float): Float =
        if (t > 0.008856f) t.pow(1f / 3f) else 7.787f * t + 16f / 116f

    private fun unpivot(t: Float): Float {
        val cubed = t * t * t
        return if (cubed > 0.008856f) cubed else (t - 16f / 116f) / 7.787f
    }

    private fun pack(roles: IntArray): ByteArray {
        val out = ByteArray(COUNT * 4)
        for (i in roles.indices) {
            val c = roles[i]
            out[i * 4] = ((c shr 16) and 0xff).toByte()
            out[i * 4 + 1] = ((c shr 8) and 0xff).toByte()
            out[i * 4 + 2] = (c and 0xff).toByte()
            out[i * 4 + 3] = ((c shr 24) and 0xff).toByte()
        }
        return out
    }
}
