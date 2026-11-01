package com.dspdispatch.relay.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.dspdispatch.relay.R
import com.dspdispatch.relay.pairing.Paired
import com.dspdispatch.relay.setup.Fix
import com.dspdispatch.relay.setup.Item
import com.dspdispatch.relay.setup.Step

/** Set up, after pairing: each step the app needs, done or with its fix, then Finish once all are. */
@Composable
fun SetupScreen(paired: Paired, items: List<Item>?, fix: (Item, Fix) -> Unit, finish: () -> Unit) = Page {
    Text(
        stringResource(R.string.set_up),
        Modifier.padding(bottom = 12.dp),
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.SemiBold,
    )
    Column(Modifier.weight(1f).verticalScroll(rememberScrollState()), Arrangement.spacedBy(12.dp)) {
        Card { DspLine(paired) }
        if (!items.isNullOrEmpty()) {
            Card {
                items.forEachIndexed { index, item ->
                    if (index > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outline)
                    StepRow(item) { fix(item, it) }
                }
            }
        }
    }
    Button(
        finish,
        Modifier.fillMaxWidth().padding(top = 12.dp).height(48.dp),
        enabled = items != null && items.all(Item::done),
    ) {
        Text(stringResource(R.string.finish))
    }
}

/** A step of Set up undone while running, above the running screen, with its fix. */
@Composable
fun UndoneBanner(item: Item, fix: (Fix) -> Unit) {
    val tones = LocalTones.current
    Surface(Modifier.fillMaxWidth().padding(top = 12.dp), RoundedCornerShape(14.dp), tones.warningSurface) {
        Row(Modifier.padding(start = 14.dp, end = 10.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(item.step.undone), Modifier.weight(1f), color = tones.warning, fontWeight = FontWeight.SemiBold)
            item.fix?.let { main ->
                Small(
                    stringResource(if (main == Fix.CONFIRM) R.string.done else R.string.fix),
                    filled = false,
                ) { fix(main) }
            }
        }
    }
}

@Composable
private fun Card(content: @Composable () -> Unit) = Surface(
    Modifier.fillMaxWidth(),
    RoundedCornerShape(18.dp),
    MaterialTheme.colorScheme.surface,
    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
) {
    Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) { content() }
}

@Composable
private fun StepRow(item: Item, fix: (Fix) -> Unit) = Row(
    Modifier.fillMaxWidth().padding(vertical = 12.dp),
    Arrangement.spacedBy(8.dp),
    Alignment.CenterVertically,
) {
    Tick(item.done)
    Text(stringResource(item.step.label), Modifier.weight(1f).padding(start = 4.dp))
    item.fixes.forEach { each -> Small(stringResource(each.label), filled = each == item.fix) { fix(each) } }
}

@Composable
private fun Tick(done: Boolean) {
    val modifier = Modifier.size(26.dp)
    if (done) {
        Box(modifier.background(LocalTones.current.success, CircleShape), Alignment.Center) {
            Image(painterResource(R.drawable.ic_check), null, Modifier.size(16.dp), colorFilter = ColorFilter.tint(MaterialTheme.colorScheme.surface))
        }
    } else {
        Box(modifier.border(1.5.dp, MaterialTheme.colorScheme.outline, CircleShape))
    }
}

/** A step's or a banner's button, small beside its label. */
@Composable
internal fun Small(text: String, filled: Boolean, click: () -> Unit) {
    val padding = PaddingValues(horizontal = 12.dp)
    val modifier = Modifier.height(32.dp)
    if (filled) {
        Button(click, modifier, contentPadding = padding) { Text(text, fontSize = 13.sp) }
    } else {
        OutlinedButton(click, modifier, contentPadding = padding) { Text(text, fontSize = 13.sp) }
    }
}

@Preview
@Composable
private fun SetupPreview() = SetupScreen(
    Paired("https://bh.dispatch.test", "Blue Heron Logistics", "phone_1", "Dispatch", 0),
    listOf(
        Item(Step.TEXTING, true, emptyList()),
        Item(Step.BATTERY, false, listOf(Fix.MAKER_BATTERY, Fix.CONFIRM)),
        Item(Step.CHAT_FEATURES, false, listOf(Fix.TEXTING_APP)),
    ),
    { _, _ -> },
    {},
)
