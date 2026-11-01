package com.dspdispatch.relay.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.dspdispatch.relay.R
import com.dspdispatch.relay.pairing.Paired
import com.dspdispatch.relay.setup.Fix
import com.dspdispatch.relay.setup.Item
import com.dspdispatch.relay.setup.Place
import com.dspdispatch.relay.setup.Step

/** Set up, after pairing: the DSP, and the one button that starts the guided walk. */
@Composable
fun SetupScreen(paired: Paired, start: () -> Unit) = Page {
    Top()
    Column(Modifier.weight(1f).fillMaxWidth(), Arrangement.Center, Alignment.CenterHorizontally) {
        Text(stringResource(R.string.set_up), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        DspLine(paired)
    }
    Big(stringResource(R.string.start), filled = true, start, Modifier.fillMaxWidth())
}

/**
 * A step of the guided walk, filling the screen: what to do, its place in the walk, and its
 * button opening Android's prompt or screen for it, with Done beside where only the person can
 * say it's done. Back leaves the walk.
 */
@Composable
fun StepScreen(place: Place, fix: (Item, Fix) -> Unit, back: () -> Unit) = Page {
    BackHandler(onBack = back)
    val item = place.item
    Text(
        stringResource(R.string.step_of, place.number, place.count),
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        style = MaterialTheme.typography.titleSmall,
    )
    LinearProgressIndicator(
        { place.number.toFloat() / place.count },
        Modifier.fillMaxWidth().padding(top = 10.dp).height(6.dp),
        trackColor = MaterialTheme.colorScheme.surfaceVariant,
    )
    Column(Modifier.weight(1f).fillMaxWidth(), Arrangement.Center, Alignment.CenterHorizontally) {
        Text(
            stringResource(place.heading),
            textAlign = TextAlign.Center,
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
        )
    }
    Row(Modifier.fillMaxWidth(), Arrangement.spacedBy(12.dp)) {
        item.fixes.forEach { each -> Big(stringResource(each.label), filled = each == item.fix, { fix(item, each) }, Modifier.weight(1f)) }
    }
}

/** A step of Set up undone while running, above the running screen, with Fix opening its guided step. */
@Composable
fun UndoneBanner(item: Item, fix: () -> Unit) {
    val tones = LocalTones.current
    Surface(Modifier.fillMaxWidth().padding(top = 12.dp), RoundedCornerShape(14.dp), tones.warningSurface) {
        Row(Modifier.padding(start = 14.dp, end = 10.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(item.step.undone), Modifier.weight(1f), color = tones.warning, fontWeight = FontWeight.SemiBold)
            OutlinedButton(fix, Modifier.height(32.dp), contentPadding = PaddingValues(horizontal = 12.dp)) {
                Text(stringResource(R.string.fix), fontSize = 13.sp)
            }
        }
    }
}

/** Set up's large button: filled for the one to tap, outlined beside it. */
@Composable
private fun Big(text: String, filled: Boolean, click: () -> Unit, modifier: Modifier) {
    val sized = modifier.height(BIG_HEIGHT.dp)
    val label = @Composable { Text(text, style = MaterialTheme.typography.titleMedium) }
    if (filled) Button(click, sized) { label() } else OutlinedButton(click, sized) { label() }
}

/** Set up's buttons' height, larger than the app's others so none is missed. */
private const val BIG_HEIGHT = 56

@Preview
@Composable
private fun SetupPreview() = SetupScreen(Paired("https://bh.dispatch.test", "Blue Heron Logistics", "phone_1", "Dispatch", 0)) {}

@Preview
@Composable
private fun StepPreview() = StepScreen(
    Place(Item(Step.CHAT_FEATURES, false, listOf(Fix.TEXTING_APP, Fix.CONFIRM)), 3, 5),
    { _, _ -> },
    {},
)
