use crate::error::AppError;
use serde_json::json;
use tauri::{AppHandle, Manager, Wry, plugin::PluginHandle};

pub struct AndroidWireless(PluginHandle<Wry>);
pub fn init() -> tauri::plugin::TauriPlugin<Wry> {
    tauri::plugin::Builder::new("wireless-network")
        .setup(|app: &AppHandle, api| {
            let handle =
                api.register_android_plugin("app.idroidloader.mobile", "WirelessNetworkPlugin")?;
            app.manage(AndroidWireless(handle));
            Ok(())
        })
        .invoke_handler(|invoke| {
            invoke
                .resolver
                .reject("Wireless network access is controlled by native pairing code");
            true
        })
        .build()
}
impl AndroidWireless {
    pub fn acquire(&self) -> Result<(), AppError> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("acquire", json!({}))
            .map(|_| ())
            .map_err(|_| {
                AppError::RemotePairing(
                    "Unable to enable Wi-Fi multicast. Connect Android to Wi-Fi and try again"
                        .into(),
                )
            })
    }
    pub fn release(&self) {
        let _ = self
            .0
            .run_mobile_plugin::<serde_json::Value>("release", json!({}));
    }
}
