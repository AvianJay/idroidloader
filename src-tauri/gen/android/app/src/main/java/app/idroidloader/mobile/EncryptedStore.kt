package app.idroidloader.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** The preferences contain authenticated ciphertext only. Keys stay in Android Keystore. */
internal class EncryptedStore(
    context: Context,
    private val alias: String = "${context.packageName}.secure-storage.v1",
    preferencesName: String = "secure-storage-v1",
) {
    private val preferences = context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)

    companion object {
        private val lock = Any()
        private const val VERSION: Byte = 1
        private const val IV_LENGTH = 12
    }

    private fun key(create: Boolean): SecretKey {
        val keystore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keystore.getKey(alias, null) as? SecretKey)?.let { return it }
        check(create) { "Encryption key unavailable" }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build())
            generateKey()
        }
    }

    private fun recordId(id: String): String = MessageDigest.getInstance("SHA-256")
        .digest(id.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) }

    private fun encrypt(id: String, value: String): String {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(create = true))
        cipher.updateAAD(id.toByteArray(Charsets.UTF_8))
        check(cipher.iv.size == IV_LENGTH)
        return Base64.encodeToString(byteArrayOf(VERSION) + cipher.iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
    }

    private fun decrypt(id: String, encoded: String): String {
        val data = Base64.decode(encoded, Base64.NO_WRAP)
        check(data.size >= 1 + IV_LENGTH + 16 && data[0] == VERSION) { "Invalid encrypted record" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(create = false), GCMParameterSpec(128, data.copyOfRange(1, 1 + IV_LENGTH)))
        cipher.updateAAD(id.toByteArray(Charsets.UTF_8))
        return String(cipher.doFinal(data.copyOfRange(1 + IV_LENGTH, data.size)), Charsets.UTF_8)
    }

    fun available(): Boolean = synchronized(lock) {
        // Probe entirely in memory; never create a test credential on disk.
        decrypt("availability", encrypt("availability", "probe")) == "probe"
    }

    fun store(id: String, value: String) = synchronized(lock) {
        check(preferences.edit().putString(recordId(id), encrypt(id, value)).commit()) { "Encrypted write failed" }
    }

    fun retrieve(id: String): String? = synchronized(lock) {
        preferences.getString(recordId(id), null)?.let { decrypt(id, it) }
    }

    fun delete(id: String): Boolean = synchronized(lock) {
        val record = recordId(id)
        val exists = preferences.contains(record)
        check(preferences.edit().remove(record).commit()) { "Encrypted deletion failed" }
        exists
    }
}
