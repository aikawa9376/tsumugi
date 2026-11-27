package io.github.aikawa9376.tsumugi.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.lifecycle.compose.LifecycleStartEffect

class MainActivity : ComponentActivity() {
    private val model: WorkModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()) {
                // An app in the background loses its socket: the work is
                // taken again, whole, each time it comes to the front.
                LifecycleStartEffect(model) {
                    model.start()
                    onStopOrDispose { model.stop() }
                }
                App(model)
            }
        }
    }
}
