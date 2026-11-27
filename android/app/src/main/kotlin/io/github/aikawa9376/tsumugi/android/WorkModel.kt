package io.github.aikawa9376.tsumugi.android

import android.app.Application
import android.os.Build
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.aikawa9376.tsumugi.core.MachineEntry
import io.github.aikawa9376.tsumugi.core.PairListener
import io.github.aikawa9376.tsumugi.core.PairStep
import io.github.aikawa9376.tsumugi.core.Phone
import io.github.aikawa9376.tsumugi.core.PhoneException
import io.github.aikawa9376.tsumugi.core.ReplyOutcome
import io.github.aikawa9376.tsumugi.core.Standing
import io.github.aikawa9376.tsumugi.core.Task
import io.github.aikawa9376.tsumugi.core.WaitEntry
import io.github.aikawa9376.tsumugi.core.WorkListener
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** How a pairing stands, as the pairing screen shows it. */
sealed interface Pairing {
    data object Asking : Pairing
    data object NotAccepting : Pairing
    data class Shown(val code: String, val mismatched: Boolean) : Pairing
    data class Failed(val why: String) : Pairing
}

data class Screen(
    val phoneKey: String = "",
    val machines: List<MachineEntry> = emptyList(),
    /** The machine whose waits are shown, by its key. */
    val selected: String? = null,
    val standing: Standing? = null,
    val waits: List<WaitEntry> = emptyList(),
    /** When the waits shown were last told, while connected. */
    val asOfMs: Long? = null,
    val drafts: Drafts = Drafts(),
    /** A pairing under way; none while none is. */
    val pairing: Pairing? = null,
    val problem: String? = null,
)

fun WaitEntry.key() = WaitKey(session, wait.toLong())

class WorkModel(application: Application) : AndroidViewModel(application) {
    private val phone = Phone(application.filesDir.resolve("tsumugi").path)
    private val state = MutableStateFlow(Screen(phoneKey = phone.key()))
    val screen: StateFlow<Screen> = state.asStateFlow()

    private var watching: Task? = null
    private var pairingTask: Task? = null

    init {
        val machines = machines()
        state.update { it.copy(machines = machines, selected = machines.firstOrNull()?.key) }
    }

    private fun machines(): List<MachineEntry> = try {
        phone.machines()
    } catch (error: PhoneException) {
        state.update { it.copy(problem = error.message) }
        emptyList()
    }

    /** Watch the machine shown, from its whole work, as the app comes to the front. */
    fun start() {
        stop()
        val key = state.value.selected ?: return
        watching = try {
            phone.watch(key, Listener(key))
        } catch (error: PhoneException) {
            state.update { it.copy(problem = error.message) }
            null
        }
    }

    /** Let the connection go, as the app leaves the front. */
    fun stop() {
        watching?.stop()
        watching?.close()
        watching = null
    }

    fun select(key: String) {
        state.update { it.copy(selected = key, waits = emptyList(), standing = null, asOfMs = null) }
        start()
    }

    private inner class Listener(private val key: String) : WorkListener {
        override fun standing(standing: Standing) {
            state.update { if (it.selected == key) it.copy(standing = standing) else it }
        }

        override fun waits(waits: List<WaitEntry>) {
            state.update {
                if (it.selected != key) {
                    it
                } else {
                    it.copy(
                        waits = waits,
                        asOfMs = System.currentTimeMillis(),
                        drafts = it.drafts.keep(waits.map(WaitEntry::key)),
                    )
                }
            }
        }
    }

    fun edit(wait: WaitEntry, text: String) {
        state.update { it.copy(drafts = it.drafts.edit(wait.key(), text)) }
    }

    fun send(wait: WaitEntry) {
        val key = state.value.selected ?: return
        val draft = state.value.drafts[wait.key()]
        if (!state.value.drafts.canSend(wait.key())) return
        state.update { it.copy(drafts = it.drafts.sending(wait.key())) }
        viewModelScope.launch {
            val outcome = withContext(Dispatchers.IO) {
                phone.reply(key, wait.session, wait.agentKey, wait.wait, draft.text)
            }
            val delivery = when (outcome) {
                is ReplyOutcome.Sent -> Delivery.Sent
                is ReplyOutcome.NotSent -> Delivery.NotSent(outcome.why)
                is ReplyOutcome.Refused -> Delivery.Refused(outcome.why)
                is ReplyOutcome.Unknown -> Delivery.Unknown(outcome.why)
                is ReplyOutcome.Held -> Delivery.Held(outcome.why)
            }
            state.update { it.copy(drafts = it.drafts.settled(wait.key(), delivery)) }
        }
    }

    fun pair(address: String) {
        cancelPairing()
        state.update { it.copy(pairing = Pairing.Asking) }
        val listener = object : PairListener {
            override fun step(step: PairStep) {
                state.update {
                    val shown = it.pairing as? Pairing.Shown
                    it.copy(
                        pairing = when (step) {
                            is PairStep.NotAccepting -> Pairing.NotAccepting
                            is PairStep.Shown -> Pairing.Shown(step.code, mismatched = shown?.mismatched == true)
                            is PairStep.Mismatched -> Pairing.Shown(shown?.code ?: "", mismatched = true)
                        },
                    )
                }
            }

            override fun paired(machine: MachineEntry) {
                state.update { it.copy(pairing = null, machines = machines(), selected = machine.key) }
                viewModelScope.launch { start() }
            }

            override fun failed(why: String) {
                state.update { it.copy(pairing = Pairing.Failed(why)) }
            }
        }
        pairingTask = try {
            phone.pair(address, Build.MODEL ?: "Android", listener)
        } catch (error: PhoneException) {
            state.update { it.copy(pairing = Pairing.Failed(error.message ?: "")) }
            null
        }
    }

    fun cancelPairing() {
        pairingTask?.stop()
        pairingTask?.close()
        pairingTask = null
        state.update { it.copy(pairing = null) }
    }

    fun readdress(key: String, address: String) = change { phone.readdress(key, address) }

    fun rename(key: String, name: String) = change { phone.rename(key, name) }

    fun forget(key: String) {
        change { phone.forget(key) }
        if (state.value.selected == key) {
            stop()
            state.update { it.copy(selected = it.machines.firstOrNull()?.key, waits = emptyList(), standing = null) }
            start()
        }
    }

    private fun change(change: () -> Unit) {
        try {
            change()
            state.update { it.copy(machines = machines(), problem = null) }
        } catch (error: PhoneException) {
            state.update { it.copy(problem = error.message) }
        }
    }

    fun dismissProblem() = state.update { it.copy(problem = null) }

    override fun onCleared() {
        stop()
        cancelPairing()
        phone.close()
    }
}
