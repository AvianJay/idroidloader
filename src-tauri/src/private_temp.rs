use isideload_vfs::{
    OpenOptionsConfig, Vfs, VfsFile, VfsMetadata, VfsPermissions, native::NativeVfs,
};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Native filesystem with a writable, app-private temporary root on Android.
/// This avoids mutating process-wide TMPDIR after the async runtime has started.
pub struct PrivateTempVfs(pub PathBuf);

macro_rules! native_methods {
    ($($name:ident ($($arg:ident : $ty:ty),*) -> $result:ty;)*) => {
        $(fn $name(&self, $($arg: $ty),*) -> $result { NativeVfs.$name($($arg),*) })*
    };
}

impl Vfs for PrivateTempVfs {
    native_methods! {
        open_file(path: &Path, options: &OpenOptionsConfig) -> io::Result<Box<dyn VfsFile>>;
        read(path: &Path) -> io::Result<Vec<u8>>;
        write(path: &Path, contents: &[u8]) -> io::Result<()>;
        copy(from: &Path, to: &Path) -> io::Result<u64>;
        rename(from: &Path, to: &Path) -> io::Result<()>;
        remove_file(path: &Path) -> io::Result<()>;
        remove_dir(path: &Path) -> io::Result<()>;
        remove_dir_all(path: &Path) -> io::Result<()>;
        create_dir(path: &Path) -> io::Result<()>;
        create_dir_all(path: &Path) -> io::Result<()>;
        read_link(path: &Path) -> io::Result<PathBuf>;
        set_permissions(path: &Path, perms: Box<dyn VfsPermissions>) -> io::Result<()>;
        metadata(path: &Path) -> io::Result<Box<dyn VfsMetadata>>;
        symlink_metadata(path: &Path) -> io::Result<Box<dyn VfsMetadata>>;
        read_dir(path: &Path) -> io::Result<Vec<PathBuf>>;
        symlink(target: &Path, link: &Path) -> io::Result<()>;
    }
    fn temp_dir(&self) -> PathBuf {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delegates_file_access_but_uses_private_temp_root() {
        let directory = tempfile::tempdir().unwrap();
        let vfs = PrivateTempVfs(directory.path().to_owned());
        let file = vfs.temp_dir().join("probe");
        vfs.write(&file, b"test").unwrap();
        assert_eq!(vfs.read(&file).unwrap(), b"test");
        assert_eq!(vfs.temp_dir(), directory.path());
    }
}
