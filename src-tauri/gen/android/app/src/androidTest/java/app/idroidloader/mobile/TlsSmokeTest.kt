package app.idroidloader.mobile

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TlsSmokeTest {
    private external fun probe(test: Int): Boolean

    @Before
    fun initialize() {
        NetworkTls.initialize(InstrumentationRegistry.getInstrumentation().targetContext)
    }

    @Test fun appleClientBuildsAndConnects() = assertTrue("Apple HTTPS probe failed", probe(0))
    @Test fun androidSystemVerifierConnects() = assertTrue("Anisette HTTPS probe failed", probe(1))
    @Test fun rejectsExpiredCertificate() = assertTrue("Expired certificate was not rejected", probe(2))
}
