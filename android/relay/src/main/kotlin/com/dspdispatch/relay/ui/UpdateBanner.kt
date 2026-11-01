package com.dspdispatch.relay.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.dspdispatch.relay.R
import com.dspdispatch.relay.update.Update

/** A newer version, above the running screen: Install, how installing it goes, or why it failed. */
@Composable
fun UpdateBanner(update: Update, install: () -> Unit) {
    val app = update.app ?: return
    Surface(Modifier.fillMaxWidth().padding(top = 12.dp), RoundedCornerShape(14.dp), MaterialTheme.colorScheme.primaryContainer) {
        Column(Modifier.padding(start = 14.dp, end = 10.dp, top = 8.dp, bottom = 8.dp)) {
            Row(Modifier.height(32.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(R.string.update_version, app.version),
                    Modifier.weight(1f),
                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                    fontWeight = FontWeight.SemiBold,
                )
                when (update) {
                    is Update.Available, is Update.Confirming -> Button(install, Modifier.height(32.dp), contentPadding = PaddingValues(horizontal = 12.dp)) {
                        Text(stringResource(R.string.install), fontSize = 13.sp)
                    }
                    is Update.Downloading -> Status(stringResource(R.string.downloading))
                    is Update.Installing -> {
                        CircularProgressIndicator(Modifier.padding(end = 8.dp).size(16.dp), strokeWidth = 2.dp)
                        Status(stringResource(R.string.installing))
                    }
                    Update.None -> Unit
                }
            }
            if (update is Update.Downloading) {
                LinearProgressIndicator({ update.fraction }, Modifier.fillMaxWidth().padding(top = 6.dp, bottom = 4.dp))
            }
            (update as? Update.Available)?.problem?.let {
                Text(stringResource(it.label), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
private fun Status(text: String) =
    Text(text, color = MaterialTheme.colorScheme.onPrimaryContainer, style = MaterialTheme.typography.bodyMedium)
