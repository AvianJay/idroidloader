use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Wry, plugin::PluginHandle};

struct AndroidUpdater(PluginHandle<Wry>);

pub fn init() -> tauri::plugin::TauriPlugin<Wry> {
    tauri::plugin::Builder::new("android-updater")
        .setup(|app: &AppHandle, api| {
            let handle =
                api.register_android_plugin("app.idroidloader.mobile", "AppUpdaterPlugin")?;
            app.manage(AndroidUpdater(handle));
            Ok(())
        })
        .invoke_handler(|invoke| {
            invoke
                .resolver
                .reject("Use the application updater commands");
            true
        })
        .build()
}

#[tauri::command]
pub async fn android_update_info(app: AppHandle) -> Result<Value, String> {
    app.state::<AndroidUpdater>()
        .0
        .run_mobile_plugin("info", json!({}))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_android_update(app: AppHandle, channel: String) -> Result<Value, String> {
    app.state::<AndroidUpdater>()
        .0
        .run_mobile_plugin("check", json!({ "channel": channel }))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_android_update(
    app: AppHandle,
    channel: String,
    version_code: u32,
) -> Result<Value, String> {
    app.state::<AndroidUpdater>()
        .0
        .run_mobile_plugin(
            "install",
            json!({ "channel": channel, "versionCode": version_code }),
        )
        .map_err(|e| e.to_string())
}
