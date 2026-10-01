package app.idroidloader.mobile

import android.content.Context
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.security.KeyStore
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** Uses isolated public fixtures only, never real saved-account preferences. */
@RunWith(AndroidJUnit4::class)
class EncryptedStoreTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val alias = "${context.packageName}.fixture.crypto"
    private val name = "fixture-crypto-test"
    private val preferences = context.getSharedPreferences(name, Context.MODE_PRIVATE)
    private fun store() = EncryptedStore(context, alias, name)

    @Before @After
    fun cleanup() {
        preferences.edit().clear().commit()
        KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry(alias) }
    }

    @Test
    fun encryptsPersistsOverwritesAndDeletes() {
        val value = "public-test-fixture-password-🔒"
        assertTrue(store().available())
        store().store("account:fixture@example.invalid", value)
        assertTrue(store().retrieve("account:fixture@example.invalid") == value)
        val first = preferences.all.values.single() as String
        assertFalse(first.contains(value))
        store().store("account:fixture@example.invalid", value)
        assertTrue(first != preferences.all.values.single())
        store().store("account:fixture@example.invalid", "updated-public-fixture")
        assertTrue(store().retrieve("account:fixture@example.invalid") == "updated-public-fixture")
        assertTrue(store().delete("account:fixture@example.invalid"))
        assertNull(store().retrieve("account:fixture@example.invalid"))
        assertFalse(store().delete("account:fixture@example.invalid"))
    }

    @Test
    fun bindsCiphertextToTheRecordAndRejectsTampering() {
        store().store("account:one", "public-fixture-one")
        store().store("signing:two", "public-fixture-two")
        val records = preferences.all.entries.toList()
        preferences.edit().putString(records[1].key, records[0].value as String).commit()
        assertTrue(runCatching { store().retrieve("account:one"); store().retrieve("signing:two") }.isFailure)
        preferences.edit().clear().commit()
        store().store("account:tamper", "public-tamper-fixture")
        val record = preferences.all.keys.single()
        val bytes = android.util.Base64.decode(preferences.getString(record, null), android.util.Base64.NO_WRAP)
        bytes[bytes.lastIndex] = (bytes.last().toInt() xor 1).toByte()
        preferences.edit().putString(record, android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)).commit()
        assertTrue(runCatching { store().retrieve("account:tamper") }.isFailure)
    }

    @Test
    fun missingKeyDoesNotSilentlyReplaceEncryptedCredentials() {
        store().store("account:fixture", "public-fixture")
        KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry(alias) }
        assertTrue(runCatching { store().retrieve("account:fixture") }.isFailure)
        assertTrue(store().delete("account:fixture"))
        assertNull(store().retrieve("account:fixture"))
    }
}
