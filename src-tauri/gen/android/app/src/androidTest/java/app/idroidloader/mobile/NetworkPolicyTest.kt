package app.idroidloader.mobile

import android.security.NetworkSecurityPolicy
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

// Run with -PidroidReleaseNetworkPolicy=true to exercise the release network policy.
@RunWith(AndroidJUnit4::class)
class NetworkPolicyTest {
    private val policy get() = NetworkSecurityPolicy.getInstance()

    @Test fun applicationHttpRemainsBlocked() {
        assertFalse(policy.isCleartextTrafficPermitted)
        for (host in listOf("ani.sidestore.app", "ani.sidestore.io", "example.com")) {
            assertFalse("Application HTTP allowed for $host", policy.isCleartextTrafficPermitted(host))
        }
    }

    @Test fun certificateRevocationHostsAreAllowed() {
        for (host in listOf("ye1.c.lencr.org", "e5.c.lencr.org", "c.pki.goog")) {
            assertTrue("CRL HTTP blocked for $host", policy.isCleartextTrafficPermitted(host))
        }
    }

    @Test fun unrelatedHostsRemainBlocked() {
        for (host in listOf("ye1.c.lencr.org.attacker.invalid", "attacker.invalid")) {
            assertFalse("Unrelated HTTP allowed for $host", policy.isCleartextTrafficPermitted(host))
        }
    }
}
