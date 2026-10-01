package app.idroidloader.mobile

import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.webkit.WebView
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

// Exercise the packaged frontend, Rust command, mDNS announcement and native
// multicast lifecycle. Only booleans leave the WebView; never read a pairing PIN.
@RunWith(AndroidJUnit4::class)
class WirelessPairSmokeTest {
    private fun webView(view: View): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) for (index in 0 until view.childCount) {
            webView(view.getChildAt(index))?.let { return it }
        }
        return null
    }

    private fun js(scenario: ActivityScenario<MainActivity>, expression: String): Boolean {
        val finished = CountDownLatch(1)
        val result = AtomicBoolean(false)
        scenario.onActivity { activity ->
            val view = webView(activity.window.decorView)
            if (view == null) finished.countDown()
            else view.evaluateJavascript("Boolean($expression)") { value ->
                result.set(value == "true")
                finished.countDown()
            }
        }
        assertTrue("WebView response timed out", finished.await(5, TimeUnit.SECONDS))
        return result.get()
    }

    private fun await(message: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(25)
        while (System.nanoTime() < deadline) {
            if (condition()) return
            Thread.sleep(200)
        }
        assertTrue(message, condition())
    }

    @Test fun advertiseAndCancelReleaseNetworkResources() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            val section = "document.querySelector('.wireless-pair')"
            await("Wireless pairing button did not load") {
                js(scenario, "$section?.querySelector('button')?.disabled === false")
            }
            try {
                assertTrue(js(scenario, "(() => { $section.querySelector('button').click(); return true; })()"))
                await("Android did not announce the wireless pairing host") {
                    js(scenario, "$section?.querySelector('[data-pairing-phase=advertising]') != null")
                }
                val screenHeld = AtomicBoolean(false)
                scenario.onActivity {
                    screenHeld.set(it.window.attributes.flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON != 0)
                }
                assertTrue("Pairing did not keep the screen awake", screenHeld.get())
            } finally {
                js(scenario, "(() => { if ($section?.querySelector('[data-pairing-phase]')) $section.querySelector('button').click(); return true; })()")
                await("Cancel did not restore the pairing button") {
                    js(scenario, "$section?.querySelector('[data-pairing-phase]') == null && $section?.querySelector('button')?.disabled === false")
                }
                await("Cancel did not release the screen/multicast scope") {
                    val released = AtomicBoolean(false)
                    scenario.onActivity {
                        released.set(it.window.attributes.flags and WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON == 0)
                    }
                    released.get()
                }
            }
        }
    }
}
