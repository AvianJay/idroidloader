use std::{path::PathBuf, sync::Mutex};

use crate::input_file::prepare_ipa;
use crate::{
    device::{DeviceInfoMutex, get_provider},
    error::AppError,
    operation::Operation,
    pairing::{get_sidestore_info, place_file},
};
use isideload::{
    sideload::{application::SpecialApp, sideloader::Sideloader},
    util::callbacks::MaxCertsCallbackBox,
};
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_fs::FilePath;

pub type SideloaderMutex = Mutex<Option<Sideloader<MaxCertsCallbackBox>>>;

pub struct SideloaderGuard<'a> {
    state: &'a SideloaderMutex,
    sideloader: Option<Sideloader<MaxCertsCallbackBox>>,
}

impl<'a> SideloaderGuard<'a> {
    pub fn take(state: &'a SideloaderMutex) -> Result<Self, AppError> {
        let mut guard = state.lock().unwrap();
        let sideloader = guard.take().ok_or(AppError::NotLoggedIn)?;
        Ok(Self {
            state,
            sideloader: Some(sideloader),
        })
    }

    pub fn get_mut(&mut self) -> &mut Sideloader<MaxCertsCallbackBox> {
        self.sideloader
            .as_mut()
            .expect("Sideloader should be present")
    }
}

impl Drop for SideloaderGuard<'_> {
    fn drop(&mut self) {
        let mut guard = self.state.lock().unwrap();
        *guard = self.sideloader.take();
    }
}

pub async fn sideload(
    device_state: State<'_, DeviceInfoMutex>,
    sideloader_state: State<'_, SideloaderMutex>,
    app_path: String,
) -> Result<Option<SpecialApp>, AppError> {
    let device = {
        let device_lock = device_state.lock().unwrap();
        match &*device_lock {
            Some(d) => d.clone(),
            None => return Err(AppError::NoDeviceSelected),
        }
    };

    let provider = get_provider(&device).await?;

    let mut sideloader = SideloaderGuard::take(&sideloader_state)?;

    let special = sideloader
        .get_mut()
        .install_app(
            &provider,
            app_path.into(),
            false,
            None::<fn(f32) -> std::future::Ready<()>>,
        )
        .await?;

    Ok(special)
}

#[tauri::command]
pub async fn sideload_operation(
    app: AppHandle,
    window: Window,
    device_state: State<'_, DeviceInfoMutex>,
    sideloader_state: State<'_, SideloaderMutex>,
    app_path: FilePath,
) -> Result<(), AppError> {
    let op = Operation::new("sideload".to_string(), &window);
    op.start("install")?;
    let prepared = op.fail_if_err(
        "install",
        tokio::task::spawn_blocking(move || prepare_ipa(&app, app_path))
            .await
            .map_err(|_| AppError::Filesystem("IPA import failed".into(), String::new()))?,
    )?;
    op.fail_if_err(
        "install",
        sideload(
            device_state,
            sideloader_state,
            prepared.path.to_string_lossy().into_owned(),
        )
        .await,
    )?;
    op.complete("install")?;
    Ok(())
}

/// Install a valid, already-signed IPA without requiring Apple ID login.
#[tauri::command]
pub async fn install_signed_operation(
    app: AppHandle,
    window: Window,
    device_state: State<'_, DeviceInfoMutex>,
    app_path: FilePath,
) -> Result<(), AppError> {
    let op = Operation::new("install_signed".into(), &window);
    op.start("install")?;
    let result = async {
        let device = device_state
            .lock()
            .unwrap()
            .clone()
            .ok_or(AppError::NoDeviceSelected)?;
        let provider = get_provider(&device).await?;
        let (prepared, application) = tokio::task::spawn_blocking(move || {
            let prepared = prepare_ipa(&app, app_path)?;
            let application =
                isideload::sideload::application::Application::new(prepared.path.clone())?;
            Ok::<_, AppError>((prepared, application))
        })
        .await
        .map_err(|_| AppError::Filesystem("IPA import failed".into(), String::new()))??;
        isideload::sideload::install::install_app(
            &provider,
            &application.bundle.bundle_dir,
            |_| {},
        )
        .await?;
        drop(prepared);
        Ok::<_, AppError>(())
    }
    .await;
    op.fail_if_err("install", result)?;
    op.complete("install")?;
    Ok(())
}

#[tauri::command]
pub async fn install_sidestore_operation(
    handle: AppHandle,
    window: Window,
    device_state: State<'_, DeviceInfoMutex>,
    sideloader_state: State<'_, SideloaderMutex>,
    nightly: bool,
    live_container: bool,
) -> Result<(), AppError> {
    let op = Operation::new("install_sidestore".to_string(), &window);
    op.start("download")?;
    // TODO: Cache & check version to avoid re-downloading
    let (filename, url) = if live_container {
        if nightly {
            (
                "LiveContainerSideStore-Nightly.ipa",
                "https://github.com/LiveContainer/LiveContainer/releases/download/nightly/LiveContainer+SideStore.ipa",
            )
        } else {
            (
                "LiveContainerSideStore.ipa",
                "https://github.com/LiveContainer/LiveContainer/releases/latest/download/LiveContainer+SideStore.ipa",
            )
        }
    } else if nightly {
        (
            "SideStore-Nightly.ipa",
            "https://github.com/SideStore/SideStore/releases/download/nightly/SideStore.ipa",
        )
    } else {
        (
            "SideStore.ipa",
            "https://github.com/SideStore/SideStore/releases/latest/download/SideStore.ipa",
        )
    };

    let cache = handle
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Filesystem("Failed to get app cache".into(), e.to_string()))?;
    op.fail_if_err(
        "download",
        tokio::fs::create_dir_all(&cache)
            .await
            .map_err(|e| AppError::Filesystem("Failed to create app cache".into(), e.to_string())),
    )?;
    let downloaded = op.fail_if_err(
        "download",
        tempfile::Builder::new()
            .prefix(filename)
            .suffix(".ipa")
            .tempfile_in(cache)
            .map_err(|e| {
                AppError::Filesystem("Failed to create download file".into(), e.to_string())
            }),
    )?;
    let dest = downloaded.path().to_owned();
    op.fail_if_err("download", download(url, &dest).await)?;
    op.move_on("download", "install")?;
    let device = {
        let device_guard = device_state.lock().unwrap();
        match &*device_guard {
            Some(d) => d.clone(),
            None => return op.fail("install", AppError::NoDeviceSelected),
        }
    };
    op.fail_if_err(
        "install",
        sideload(
            device_state,
            sideloader_state,
            dest.to_string_lossy().to_string(),
        )
        .await,
    )?;
    op.move_on("install", "pairing")?;
    let sidestore_info =
        op.fail_if_err("pairing", get_sidestore_info(&device, live_container).await)?;
    if let Some(info) = sidestore_info {
        let provider = op.fail_if_err("pairing", get_provider(&device).await)?;

        op.fail_if_err(
            "pairing",
            place_file(device.pairing, &provider, info.bundle_id, info.path).await,
        )?;
    } else {
        return op.fail(
            "pairing",
            AppError::HouseArrest(
                "SideStore's not found".into(),
                "The device did not report SideStore's bundle ID as installed".into(),
            ),
        );
    }

    op.complete("pairing")?;
    Ok(())
}

pub async fn download(url: impl AsRef<str>, dest: &PathBuf) -> Result<(), AppError> {
    use tokio::io::AsyncWriteExt;
    let mut response = reqwest::get(url.as_ref())
        .await
        .map_err(|e| AppError::Download(e.to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::Download(format!(
            "Failed to download file: HTTP {}",
            response.status()
        )));
    }

    let mut output = tokio::fs::File::create(dest).await.map_err(|e| {
        AppError::Filesystem("Failed to write downloaded file".into(), e.to_string())
    })?;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| AppError::Download(e.to_string()))?
    {
        output.write_all(&chunk).await.map_err(|e| {
            AppError::Filesystem("Failed to write downloaded file".into(), e.to_string())
        })?;
    }

    Ok(())
}
