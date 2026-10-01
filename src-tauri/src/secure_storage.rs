use std::sync::atomic::{AtomicBool, Ordering};

use isideload::util::storage::SideloadingStorage;
#[cfg(desktop)]
use isideload::util::{fs_storage::FsStorage, keyring_storage::KeyringStorage};
use tauri::AppHandle;
#[cfg(desktop)]
use tauri::Manager;
use tracing::warn;

use crate::error::AppError;

static FORCE_DISABLE_KEYRING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn force_disable_keyring(force: bool) {
    FORCE_DISABLE_KEYRING.store(force, Ordering::Relaxed);

    if force {
        warn!("Keyring has been forcefully disabled by the user.");
    } else {
        let available = check_keyring_available();
        if !available {
            warn!("Keyring is not available and cannot be enabled.");
        }
    }
}

#[tauri::command]
pub fn keyring_available() -> bool {
    !FORCE_DISABLE_KEYRING.load(Ordering::Relaxed) && check_keyring_available()
}

fn check_keyring_available() -> bool {
    #[cfg(mobile)]
    return false;
    #[cfg(desktop)]
    {
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
    // Until a native Keystore backend is added, keep signing/authentication material
    // in process memory on mobile instead of falling back to plaintext files.
    #[cfg(mobile)]
    {
        let _ = app;
        return Ok(Box::new(isideload::util::storage::InMemoryStorage::new()));
    }
    #[cfg(desktop)]
    {
        if keyring_available() {
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
