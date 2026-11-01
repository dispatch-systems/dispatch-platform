package com.dspdispatch.relay.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

// Dispatch's own, as core/shell/frontend/styles/tokens.css has them.
private val lightScheme = lightColorScheme(
    primary = Color(0xFF2055ED),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFEDF1FD), // --selection
    onPrimaryContainer = Color(0xFF2055ED),
    background = Color.White,
    onBackground = Color(0xFF171B25), // --foreground
    surface = Color.White,
    onSurface = Color(0xFF171B25),
    surfaceVariant = Color(0xFFEDF0F4), // --muted
    onSurfaceVariant = Color(0xFF626B7A), // --muted-foreground
    outline = Color(0xFFE3E7EE), // --border
    error = Color(0xFFB42332), // --destructive
)
private val darkScheme = darkColorScheme(
    primary = Color(0xFF8EAAFF),
    onPrimary = Color(0xFF11151D),
    primaryContainer = Color(0xFF253251),
    onPrimaryContainer = Color(0xFF8EAAFF),
    background = Color(0xFF11151D),
    onBackground = Color(0xFFEDF1F7),
    surface = Color(0xFF11151D),
    onSurface = Color(0xFFEDF1F7),
    surfaceVariant = Color(0xFF252E3D),
    onSurfaceVariant = Color(0xFFA6B1C2),
    outline = Color(0xFF303949),
    error = Color(0xFFF49AAA),
)

/** Dispatch's colors Material's scheme has no place for: done, and needing attention. */
@Immutable
data class Tones(val success: Color, val warning: Color, val warningSurface: Color)

private val lightTones = Tones(success = Color(0xFF16814B), warning = Color(0xFF956009), warningSurface = Color(0xFFFFF8EB))
private val darkTones = Tones(success = Color(0xFF6ED39A), warning = Color(0xFFE9B963), warningSurface = Color(0xFF32291C))

/** The theme's [Tones]. */
val LocalTones = staticCompositionLocalOf { lightTones }

@Composable
fun RelayTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    CompositionLocalProvider(LocalTones provides if (dark) darkTones else lightTones) {
        MaterialTheme(colorScheme = if (dark) darkScheme else lightScheme, content = content)
    }
}
