use std::sync::atomic::{AtomicBool, Ordering};

use isideload::util::storage::SideloadingStorage;
#[cfg(desktop)]
use isideload::util::{fs_storage::FsStorage, keyring_storage::KeyringStorage};
use tauri::AppHandle;
#[cfg(any(desktop, target_os = "android"))]
use tauri::Manager;
use tracing::warn;

use crate::error::AppError;

static FORCE_DISABLE_KEYRING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn force_disable_keyring(app: AppHandle, force: bool) {
    FORCE_DISABLE_KEYRING.store(force, Ordering::Relaxed);

    if force {
        warn!("Keyring has been forcefully disabled by the user.");
    } else {
        let available = check_keyring_available(&app);
        if !available {
            warn!("Keyring is not available and cannot be enabled.");
        }
    }
}

#[tauri::command]
pub async fn keyring_available(app: AppHandle) -> bool {
    storage_available(&app)
}

pub(crate) fn storage_available(app: &AppHandle) -> bool {
    !FORCE_DISABLE_KEYRING.load(Ordering::Relaxed) && check_keyring_available(app)
}

fn check_keyring_available(app: &AppHandle) -> bool {
    #[cfg(target_os = "android")]
    return app
        .state::<crate::android_storage::AndroidStorage>()
        .available();
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        return false;
    }
    #[cfg(desktop)]
    {
        let _ = app;
        let entry = keyring::Entry::new("iloader", "test");
        if let Ok(entry) = entry {
            return entry.set_password("test").is_ok() && entry.get_password().is_ok();
        }
        false
    }
}

pub fn create_sideloading_storage(
    app: &AppHandle,
) -> Result<Box<dyn SideloadingStorage>, AppError> {
    #[cfg(target_os = "android")]
    {
        if storage_available(app) {
            return Ok(Box::new(crate::android_storage::AndroidSideloadingStorage(
                app.clone(),
            )));
        }
        return Ok(Box::new(isideload::util::storage::InMemoryStorage::new()));
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app;
        return Ok(Box::new(isideload::util::storage::InMemoryStorage::new()));
    }
    #[cfg(desktop)]
    {
        if storage_available(app) {
            Ok(Box::new(KeyringStorage::new("iloader".to_string())))
        } else {
            warn!(
                "Keyring is not available, falling back to filesystem storage for sideloading data. This is insecure!"
            );
            Ok(Box::new(FsStorage::new(
                app.path().app_data_dir().map_err(|e| {
                    AppError::Misc(format!("Failed to get app data directory: {:?}", e))
                })?,
            )))
        }
    }
}

pub fn save_password(app: &AppHandle, email: &str, password: &str) -> Result<(), AppError> {
    if !storage_available(app) {
        return Err(AppError::Keyring(
            "Secure credential storage is unavailable on this device".into(),
        ));
    }
    #[cfg(target_os = "android")]
    return app
        .state::<crate::android_storage::AndroidStorage>()
        .save(&format!("account:{email}"), password);
    #[cfg(not(target_os = "android"))]
    keyring::Entry::new("iloader", email)
        .and_then(|entry| entry.set_password(password))
        .map_err(|_| AppError::Keyring("Failed to save credentials".into()))
}

pub fn load_password(app: &AppHandle, email: &str) -> Result<String, AppError> {
    if !storage_available(app) {
        return Err(AppError::Keyring(
            "Secure credential storage is unavailable on this device".into(),
        ));
    }
    #[cfg(target_os = "android")]
    return app
        .state::<crate::android_storage::AndroidStorage>()
        .load(&format!("account:{email}"))?
        .ok_or_else(|| {
            AppError::Keyring(
                "Saved password was not found. Delete this account and sign in again".into(),
            )
        });
    #[cfg(not(target_os = "android"))]
    keyring::Entry::new("iloader", email)
        .and_then(|entry| entry.get_password())
        .map_err(|_| AppError::Keyring("Failed to get saved credentials".into()))
}

pub fn delete_password(app: &AppHandle, email: &str) -> Result<(), AppError> {
    remove_entry(app, "account", email).map(|_| ())
}

pub fn delete_anisette(app: &AppHandle) -> Result<bool, AppError> {
    remove_entry(app, "signing", "anisette_state")
}

fn remove_entry(app: &AppHandle, namespace: &str, key: &str) -> Result<bool, AppError> {
    #[cfg(target_os = "android")]
    return app
        .state::<crate::android_storage::AndroidStorage>()
        .remove(&format!("{namespace}:{key}"));
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, namespace);
        match keyring::Entry::new("iloader", key).and_then(|entry| entry.delete_credential()) {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(_) => Err(AppError::Keyring(
                "Failed to delete saved credentials".into(),
            )),
        }
    }
}
