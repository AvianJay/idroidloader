use isideload::util::storage::SideloadingStorage;
use rootcause::Report;
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager, Wry, plugin::PluginHandle};

use crate::error::AppError;

pub struct AndroidStorage(PluginHandle<Wry>);

#[derive(Deserialize)]
struct Availability {
    available: bool,
}

#[derive(Deserialize)]
struct StoredValue {
    value: Option<String>,
}

#[derive(Deserialize)]
struct DeletedValue {
    removed: bool,
}

pub fn init() -> tauri::plugin::TauriPlugin<Wry> {
    tauri::plugin::Builder::new("secure-storage")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin("app.idroidloader.mobile", "SecureStoragePlugin")?;
            app.manage(AndroidStorage(handle));
            Ok(())
        })
        // Only Rust account/storage code may access secrets. Never forward a
        // frontend invocation to the native plugin's retrieve command.
        .invoke_handler(|invoke| {
            invoke
                .resolver
                .reject("Secure storage is only accessible from native account code");
            true
        })
        .build()
}

impl AndroidStorage {
    pub fn available(&self) -> bool {
        self.0
            .run_mobile_plugin::<Availability>("available", json!({}))
            .is_ok_and(|result| result.available)
    }

    pub fn save(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("store", json!({ "key": key, "value": value }))
            .map(|_| ())
            // Bridge errors can contain input data; use fixed errors instead.
            .map_err(|_| {
                AppError::Keyring("Unable to encrypt and save credentials on this device".into())
            })
    }

    pub fn load(&self, key: &str) -> Result<Option<String>, AppError> {
        self.0
            .run_mobile_plugin::<StoredValue>("retrieve", json!({ "key": key }))
            .map(|result| result.value)
            .map_err(|_| AppError::Keyring("Unable to decrypt saved credentials. Delete the saved account and sign in again".into()))
    }

    pub fn remove(&self, key: &str) -> Result<bool, AppError> {
        self.0
            .run_mobile_plugin::<DeletedValue>("delete", json!({ "key": key }))
            .map(|result| result.removed)
            .map_err(|_| AppError::Keyring("Unable to delete saved credentials".into()))
    }
}

pub struct AndroidSideloadingStorage(pub AppHandle);

impl SideloadingStorage for AndroidSideloadingStorage {
    fn store(&self, key: &str, value: &str) -> Result<(), Report> {
        Ok(self
            .0
            .state::<AndroidStorage>()
            .save(&format!("signing:{key}"), value)
            .map_err(Report::new)?)
    }

    fn retrieve(&self, key: &str) -> Result<Option<String>, Report> {
        Ok(self
            .0
            .state::<AndroidStorage>()
            .load(&format!("signing:{key}"))
            .map_err(Report::new)?)
    }

    fn delete(&self, key: &str) -> Result<(), Report> {
        Ok(self
            .0
            .state::<AndroidStorage>()
            .remove(&format!("signing:{key}"))
            .map(|_| ())
            .map_err(Report::new)?)
    }
}
