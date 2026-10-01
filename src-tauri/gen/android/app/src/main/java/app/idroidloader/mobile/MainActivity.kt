package app.idroidloader.mobile

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    NetworkTls.initialize(applicationContext)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
