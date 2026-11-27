package io.github.aikawa9376.tsumugi.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.aikawa9376.tsumugi.core.MachineEntry
import io.github.aikawa9376.tsumugi.core.Standing
import io.github.aikawa9376.tsumugi.core.WaitAnswer
import io.github.aikawa9376.tsumugi.core.WaitEntry
import kotlinx.coroutines.delay

@Composable
fun App(model: WorkModel) {
    val screen by model.screen.collectAsStateWithLifecycle()
    var adding by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<MachineEntry?>(null) }
    val pairing = screen.pairing
    when {
        pairing != null || adding || screen.machines.isEmpty() -> PairScreen(
            pairing = pairing,
            phoneKey = screen.phoneKey,
            canCancel = screen.machines.isNotEmpty(),
            onPair = model::pair,
            onCancel = {
                model.cancelPairing()
                adding = false
            },
            onDone = { adding = false },
        )
        else -> WaitsScreen(
            screen = screen,
            onSelect = model::select,
            onAdd = { adding = true },
            onEdit = { editing = it },
            onDraft = model::edit,
            onSend = model::send,
        )
    }
    editing?.let { machine ->
        MachineDialog(
            machine = machine,
            phoneKey = screen.phoneKey,
            onSave = { name, address ->
                model.rename(machine.key, name)
                model.readdress(machine.key, address)
                editing = null
            },
            onForget = {
                model.forget(machine.key)
                editing = null
            },
            onDismiss = { editing = null },
        )
    }
    screen.problem?.let { problem ->
        AlertDialog(
            onDismissRequest = model::dismissProblem,
            confirmButton = { TextButton(onClick = model::dismissProblem) { Text("OK") } },
            text = { Text(problem) },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PairScreen(
    pairing: Pairing?,
    phoneKey: String,
    canCancel: Boolean,
    onPair: (String) -> Unit,
    onCancel: () -> Unit,
    onDone: () -> Unit,
) {
    var address by remember { mutableStateOf("") }
    LaunchedEffect(pairing) {
        if (pairing == null && address.isNotEmpty()) onDone()
    }
    Scaffold(topBar = { TopAppBar(title = { Text("Pair with a work machine") }) }) { padding ->
        Column(
            modifier = Modifier.padding(padding).padding(16.dp).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                "On the work machine, turn the gateway on and run `tsumugi remote pair`. " +
                    "Give its address here, on the same network or over your VPN.",
            )
            OutlinedTextField(
                value = address,
                onValueChange = { address = it },
                label = { Text("Address, such as work-box.local or 100.64.0.7:47130") },
                singleLine = true,
                enabled = pairing == null || pairing is Pairing.Failed,
                modifier = Modifier.fillMaxWidth(),
            )
            when (pairing) {
                null, is Pairing.Failed -> {
                    if (pairing is Pairing.Failed) {
                        Text(pairing.why, color = MaterialTheme.colorScheme.error)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { onPair(address) }, enabled = address.isNotBlank()) { Text("Pair") }
                        if (canCancel) TextButton(onClick = onCancel) { Text("Cancel") }
                    }
                }
                Pairing.Asking -> {
                    Text("Reaching the work machine…")
                    TextButton(onClick = onCancel) { Text("Cancel") }
                }
                Pairing.NotAccepting -> {
                    Text("The work machine is not accepting pairing requests. Run `tsumugi remote pair` there; this phone keeps asking.")
                    TextButton(onClick = onCancel) { Text("Cancel") }
                }
                is Pairing.Shown -> {
                    if (pairing.mismatched) {
                        Text("The code typed did not match. Type this new one.", color = MaterialTheme.colorScheme.error)
                    }
                    Text("Type this code on the work machine:")
                    Text(pairing.code, fontSize = 40.sp, fontFamily = FontFamily.Monospace)
                    TextButton(onClick = onCancel) { Text("Cancel") }
                }
            }
            Spacer(Modifier.padding(8.dp))
            Text("This phone's key", style = MaterialTheme.typography.labelMedium)
            SelectionContainer {
                Text(phoneKey, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WaitsScreen(
    screen: Screen,
    onSelect: (String) -> Unit,
    onAdd: () -> Unit,
    onEdit: (MachineEntry) -> Unit,
    onDraft: (WaitEntry, String) -> Unit,
    onSend: (WaitEntry) -> Unit,
) {
    val machine = screen.machines.firstOrNull { it.key == screen.selected } ?: screen.machines.first()
    var menu by remember { mutableStateOf(false) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    TextButton(onClick = { menu = true }) {
                        Text(machine.name, style = MaterialTheme.typography.titleLarge)
                    }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        screen.machines.forEach {
                            DropdownMenuItem(text = { Text(it.name) }, onClick = {
                                menu = false
                                onSelect(it.key)
                            })
                        }
                        DropdownMenuItem(text = { Text("Edit ${machine.name}…") }, onClick = {
                            menu = false
                            onEdit(machine)
                        })
                        DropdownMenuItem(text = { Text("Pair another machine…") }, onClick = {
                            menu = false
                            onAdd()
                        })
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            StandingLine(screen.standing, screen.asOfMs)
            if (screen.waits.isEmpty()) {
                Text(
                    if (screen.standing is Standing.Live) "No session waits on you." else "",
                    modifier = Modifier.padding(16.dp),
                )
            }
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(screen.waits, key = { it.session }) { wait ->
                    WaitCard(wait, screen.drafts, onDraft, onSend)
                }
            }
        }
    }
}

@Composable
private fun StandingLine(standing: Standing?, asOfMs: Long?) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            now = System.currentTimeMillis()
        }
    }
    val asOf = asOfMs?.let { " Shown as of ${age(now, it)}." } ?: ""
    val (text, error) = when (standing) {
        null, Standing.Connecting -> "Connecting…$asOf" to false
        Standing.Live -> return
        is Standing.Lost -> "Not connected: ${standing.why}. Trying again.$asOf" to false
        is Standing.Refused -> "${standing.why}.$asOf" to true
    }
    Text(
        text,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        style = MaterialTheme.typography.bodyMedium,
        color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun WaitCard(
    wait: WaitEntry,
    drafts: Drafts,
    onDraft: (WaitEntry, String) -> Unit,
    onSend: (WaitEntry) -> Unit,
) {
    Card(
        modifier = Modifier.padding(horizontal = 12.dp).fillMaxWidth(),
        colors = CardDefaults.cardColors(),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(wait.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Spacer(Modifier.width(8.dp))
                Text("${wait.workspace} · ${wait.agent}", style = MaterialTheme.typography.labelSmall)
            }
            wait.lastUser?.let { Said("You", it) }
            wait.lastAgent?.let { Said(wait.agent, it) }
            when (val answer = wait.answer) {
                is WaitAnswer.AtWindow -> Text(
                    "Answer at a window: ${answer.asks}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.tertiary,
                )
                is WaitAnswer.Reply -> Reply(wait, drafts, onDraft, onSend)
            }
        }
    }
}

@Composable
private fun Said(who: String, text: String) {
    Column {
        Text(who, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(text, style = MaterialTheme.typography.bodyMedium, maxLines = 6, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun Reply(
    wait: WaitEntry,
    drafts: Drafts,
    onDraft: (WaitEntry, String) -> Unit,
    onSend: (WaitEntry) -> Unit,
) {
    val draft = drafts[wait.key()]
    OutlinedTextField(
        value = draft.text,
        onValueChange = { onDraft(wait, it) },
        label = { Text("Reply") },
        enabled = !draft.sending,
        modifier = Modifier.fillMaxWidth(),
    )
    when (val note = draft.note) {
        null, Delivery.Sent -> {}
        is Delivery.NotSent -> Note("Not sent: ${note.why}", error = true)
        is Delivery.Refused -> Note("Not typed: ${note.why}", error = true)
        is Delivery.Unknown -> Note("Reply unconfirmed (${note.why}). Look at the session before replying again.", error = true)
        is Delivery.Held -> Note(note.why, error = true)
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Button(onClick = { onSend(wait) }, enabled = drafts.canSend(wait.key())) {
            Text(if (draft.sending) "Sending…" else "Send")
        }
    }
}

@Composable
private fun Note(text: String, error: Boolean) {
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun MachineDialog(
    machine: MachineEntry,
    phoneKey: String,
    onSave: (String, String) -> Unit,
    onForget: () -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf(machine.name) }
    var address by remember { mutableStateOf(machine.address) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Work machine") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text("Name") }, singleLine = true)
                OutlinedTextField(value = address, onValueChange = { address = it }, label = { Text("Address") }, singleLine = true)
                Text("Its key", style = MaterialTheme.typography.labelMedium)
                SelectionContainer { Text(machine.key, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                Text("This phone's key", style = MaterialTheme.typography.labelMedium)
                SelectionContainer { Text(phoneKey, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
                Text(
                    "Forgetting the machine here does not revoke this phone there: run `tsumugi remote revoke` on it.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = { TextButton(onClick = { onSave(name, address) }) { Text("Save") } },
        dismissButton = {
            Row {
                TextButton(onClick = onForget) { Text("Forget") }
                TextButton(onClick = onDismiss) { Text("Cancel") }
            }
        },
    )
}
