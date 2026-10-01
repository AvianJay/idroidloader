package app.idroidloader.mobile

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.security.KeyStore
import org.junit.Assert.*
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters

/** Run the two methods in separate instrumentation processes to verify a cold restart. */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class EncryptedStoreRestartTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val alias = "${context.packageName}.fixture.restart"
    private val name = "fixture-restart-test"
    private fun store() = EncryptedStore(context, alias, name)

    @Test
    fun aWriteFixture() {
        context.getSharedPreferences(name, Context.MODE_PRIVATE).edit().clear().commit()
        store().store("account:fixture@example.invalid", "public-restart-fixture")
        store().store("signing:anisette_state", "public-anisette-fixture")
    }

    @Test
    fun bReadAndDeleteFixture() {
        try {
            assertTrue(store().retrieve("account:fixture@example.invalid") == "public-restart-fixture")
            assertTrue(store().retrieve("signing:anisette_state") == "public-anisette-fixture")
            assertTrue(store().delete("account:fixture@example.invalid"))
            assertNull(store().retrieve("account:fixture@example.invalid"))
        } finally {
            context.getSharedPreferences(name, Context.MODE_PRIVATE).edit().clear().commit()
            KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry(alias) }
        }
    }
}
