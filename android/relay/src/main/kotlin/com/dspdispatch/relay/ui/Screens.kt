package com.dspdispatch.relay.ui

import android.text.format.DateFormat
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.dspdispatch.relay.BuildConfig
import com.dspdispatch.relay.R
import com.dspdispatch.relay.connection.Link
import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.pairing.Paired
import com.dspdispatch.relay.setup.Fix
import com.dspdispatch.relay.setup.Item
import com.dspdispatch.relay.setup.Step
import com.dspdispatch.relay.texts.Today
import com.dspdispatch.relay.update.Update
import java.util.Date

/** Never paired: the app's mark and name, and scanning a DSP's pairing code. */
@Composable
fun StartScreen(scan: () -> Unit, pairing: Boolean, problem: Int?) = Page {
    Column(Modifier.weight(1f).fillMaxWidth(), Arrangement.Center, Alignment.CenterHorizontally) {
        Mark(72)
        Spacer(Modifier.height(14.dp))
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
    }
    ScanButton(scan, pairing, problem)
}

/** Removed by its DSP: pairs again with a new code. */
@Composable
fun RemovedScreen(dsp: String, scan: () -> Unit, pairing: Boolean, problem: Int?) = Page {
    Top()
    Column(Modifier.weight(1f).fillMaxWidth(), Arrangement.Center, Alignment.CenterHorizontally) {
        Text(stringResource(R.string.removed), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
        Text(dsp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    ScanButton(scan, pairing, problem)
}

/**
 * Paired: a step of Set up undone, if one is, a newer version, if there's one, the DSP, how the
 * line to it stands, today's texts, the app's version, and the switch of [mmsWhenBusy], which
 * [setMmsWhenBusy] turns.
 */
@Composable
fun ConnectedScreen(
    paired: Paired,
    link: Link,
    today: Today,
    undone: Item?,
    fix: (Item, Fix) -> Unit,
    mmsWhenBusy: Boolean,
    setMmsWhenBusy: (Boolean) -> Unit,
    update: Update,
    install: () -> Unit,
) = Page {
    Top()
    undone?.let { item -> UndoneBanner(item) { fix(item, it) } }
    UpdateBanner(update, install)
    DspLine(paired)
    Surface(Modifier.fillMaxWidth(), RoundedCornerShape(12.dp), MaterialTheme.colorScheme.surfaceVariant) {
        Column(Modifier.padding(16.dp)) {
            val (state, since) = when (link) {
                Link.Connecting -> stringResource(R.string.connecting) to null
                is Link.Connected -> stringResource(R.string.connected) to stringResource(R.string.since, time(link.since))
                is Link.Reconnecting -> stringResource(R.string.reconnecting) to stringResource(R.string.offline_since, time(link.since))
            }
            Text(state, color = MaterialTheme.colorScheme.onSurface, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            if (since != null) Text(since, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    TodayCard(today)
    SwitchCard(R.string.mms_when_busy, mmsWhenBusy, setMmsWhenBusy)
}

/** A switch with its [label], [on] or off, which [set] turns: the whole card turns it, read as one. */
@Composable
private fun SwitchCard(@StringRes label: Int, on: Boolean, set: (Boolean) -> Unit) = Surface(
    Modifier.fillMaxWidth().padding(top = 12.dp),
    RoundedCornerShape(12.dp),
    MaterialTheme.colorScheme.surfaceVariant,
) {
    Row(
        Modifier.toggleable(on, role = Role.Switch, onValueChange = set).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(label), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
        Switch(on, onCheckedChange = null)
    }
}

/** The DSP, and the phone's name at it. */
@Composable
internal fun DspLine(paired: Paired) = Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
    Box(
        Modifier.size(36.dp).background(MaterialTheme.colorScheme.primaryContainer, RoundedCornerShape(10.dp)),
        Alignment.Center,
    ) {
        Text(initials(paired.dsp), color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
    }
    Column(Modifier.padding(start = 10.dp)) {
        Text(paired.dsp, fontWeight = FontWeight.SemiBold)
        Text(paired.phoneName, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun TodayCard(today: Today) = Surface(
    Modifier.fillMaxWidth().padding(top = 12.dp),
    RoundedCornerShape(12.dp),
    MaterialTheme.colorScheme.surfaceVariant,
) {
    Column(Modifier.padding(16.dp)) {
        Text(stringResource(R.string.today), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
        Row(Modifier.padding(vertical = 12.dp), Arrangement.spacedBy(8.dp)) {
            Count(today.received, R.string.texts_in)
            Count(today.sent, R.string.texts_out)
            Count(today.waiting, R.string.texts_waiting)
        }
        Detail(stringResource(R.string.last_text), today.latest?.let { time(it) } ?: NONE)
        Detail(stringResource(R.string.version), BuildConfig.VERSION_NAME)
    }
}

@Composable
private fun RowScope.Count(count: Int, @StringRes label: Int) = Column(
    Modifier.weight(1f).background(MaterialTheme.colorScheme.surface, RoundedCornerShape(10.dp)).padding(vertical = 10.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
) {
    Text("$count", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
    Text(stringResource(label), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun Detail(label: String, value: String) = Row(Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
    Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
    Text(value, style = MaterialTheme.typography.bodyMedium)
}

/** What a detail shows while there's nothing to show. */
private const val NONE = "–"

/** A screen of the app: its theme, edge to edge clear of the system's bars. */
@Composable
internal fun Page(content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) = RelayTheme {
    Surface(Modifier.fillMaxSize()) {
        Column(Modifier.safeDrawingPadding().padding(20.dp), content = content)
    }
}

@Composable
private fun Top() = Row(verticalAlignment = Alignment.CenterVertically) {
    Mark(28)
    Text(
        stringResource(R.string.app_name),
        Modifier.padding(start = 10.dp),
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.SemiBold,
    )
}

@Composable
private fun Mark(size: Int) = Image(
    painterResource(R.drawable.ic_notification),
    contentDescription = null,
    modifier = Modifier.size(size.dp),
    colorFilter = ColorFilter.tint(MaterialTheme.colorScheme.primary),
)

@Composable
private fun ScanButton(scan: () -> Unit, pairing: Boolean, problem: Int?) = Column(
    Modifier.fillMaxWidth(),
    horizontalAlignment = Alignment.CenterHorizontally,
) {
    if (problem != null) {
        Text(stringResource(problem), Modifier.padding(bottom = 12.dp), color = MaterialTheme.colorScheme.error)
    }
    Button(scan, Modifier.fillMaxWidth().height(48.dp), enabled = !pairing) {
        if (pairing) {
            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        } else {
            Text(stringResource(R.string.scan_code))
        }
    }
}

/** The scanner, filling the screen, with a way back. */
@Composable
fun ScanScreen(read: (String) -> Unit, back: () -> Unit) = Page {
    BackHandler(onBack = back)
    Scanner(read, Modifier.weight(1f).fillMaxWidth())
    OutlinedButton(back, Modifier.fillMaxWidth().padding(top = 12.dp).height(48.dp)) {
        Text(stringResource(R.string.back))
    }
}

/** Asks for the camera, which scanning needs. */
@Composable
fun CameraScreen(allow: () -> Unit, back: () -> Unit) = Page {
    BackHandler(onBack = back)
    Column(Modifier.weight(1f).fillMaxWidth(), Arrangement.Center, Alignment.CenterHorizontally) {
        Text(stringResource(R.string.camera_needed), style = MaterialTheme.typography.titleMedium)
    }
    Button(allow, Modifier.fillMaxWidth().height(48.dp)) { Text(stringResource(R.string.allow)) }
    OutlinedButton(back, Modifier.fillMaxWidth().padding(top = 12.dp).height(48.dp)) { Text(stringResource(R.string.back)) }
}

/** A DSP's initials, as Dispatch shows them: its name's first two words' first letters. */
internal fun initials(name: String) =
    name.split(' ').filter(String::isNotBlank).take(2).joinToString("") { it.first().uppercase() }

@Composable
private fun time(at: Long) = DateFormat.getTimeFormat(LocalContext.current).format(Date(at))

@Preview
@Composable
private fun ConnectedPreview() = ConnectedScreen(
    Paired("https://bh.dispatch.test", "Blue Heron Logistics", "phone_1", "Dispatch", 0),
    Link.Connected(0),
    Today(received = 42, sent = 37, waiting = 0, latest = 0),
    Item(Step.BATTERY, false, listOf(Fix.BATTERY_REQUEST)),
    { _, _ -> },
    mmsWhenBusy = true,
    setMmsWhenBusy = {},
    update = Update.Available(AppVersion("1.0.5", 5)),
    install = {},
)
