use std::{io::Read, path::PathBuf};

use tauri::{AppHandle, Manager};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

use crate::error::AppError;

/// Open regular paths and Android document-provider URIs without exposing contents to JS.
pub fn open_input(app: &AppHandle, path: FilePath) -> Result<std::fs::File, AppError> {
    let mut options = OpenOptions::new();
    options.read(true);
    app.fs().open(path, options).map_err(|_| {
        AppError::Filesystem(
            "Unable to open the selected file".into(),
            "Select it again using the file picker".into(),
        )
    })
}

pub fn read_pairing_input(app: &AppHandle, path: FilePath) -> Result<Vec<u8>, AppError> {
    const MAX_PAIRING_SIZE: u64 = 1024 * 1024;
    let mut bytes = Vec::new();
    open_input(app, path)?
        .take(MAX_PAIRING_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            AppError::Filesystem(
                "Unable to read pairing file".into(),
                "Select a local copy of the file".into(),
            )
        })?;
    if bytes.len() as u64 > MAX_PAIRING_SIZE {
        return Err(AppError::LockdownPairing(
            "Pairing file is too large".into(),
            "Maximum size is 1 MiB".into(),
        ));
    }
    Ok(bytes)
}

pub struct PreparedIpa {
    pub path: PathBuf,
    _temporary: Option<tempfile::NamedTempFile>,
}

/// Stream Android document URIs into private cache, never through frontend memory.
pub fn prepare_ipa(app: &AppHandle, path: FilePath) -> Result<PreparedIpa, AppError> {
    if let FilePath::Path(path) = &path {
        return Ok(PreparedIpa {
            path: path.clone(),
            _temporary: None,
        });
    }
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|_| AppError::Filesystem("Unable to access app cache".into(), String::new()))?;
    std::fs::create_dir_all(&cache)
        .map_err(|_| AppError::Filesystem("Unable to create app cache".into(), String::new()))?;
    let mut temporary = tempfile::Builder::new()
        .prefix("import-")
        .suffix(".ipa")
        .tempfile_in(cache)
        .map_err(|_| {
            AppError::Filesystem(
                "Unable to create temporary IPA".into(),
                "Check available storage".into(),
            )
        })?;
    let mut source = open_input(app, path)?;
    std::io::copy(&mut source, &mut temporary).map_err(|_| {
        AppError::Filesystem(
            "Unable to import IPA".into(),
            "Check available storage and download the file locally".into(),
        )
    })?;
    Ok(PreparedIpa {
        path: temporary.path().to_owned(),
        _temporary: Some(temporary),
    })
}
