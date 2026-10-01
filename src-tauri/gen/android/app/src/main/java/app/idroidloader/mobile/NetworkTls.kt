package app.idroidloader.mobile

import android.content.Context

object NetworkTls {
    init { System.loadLibrary("iloader_lib") }
    @JvmStatic external fun initialize(context: Context)
}
