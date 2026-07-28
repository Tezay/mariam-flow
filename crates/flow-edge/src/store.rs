//! Durable writes shared by everything the appliance persists.
//!
//! An appliance loses power without warning — someone unplugs it, or the
//! building does. Every file the daemon owns is therefore written the same
//! way: to a temporary neighbour first, then moved into place. `rename`
//! within one filesystem is atomic, so at any instant a reader sees either
//! the whole previous version or the whole new one, never a half-written
//! file. Writing in place offers no such guarantee, and a truncated
//! configuration is an appliance that will not boot.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::StoreError;

/// Writes `contents` to `path` atomically, restricting it to its owner.
///
/// Missing parent directories are created. The file is restricted before
/// it is moved into place, so it is never briefly world-readable — the
/// files written this way carry Wi-Fi passphrases and credential hashes.
///
/// # Errors
///
/// [`StoreError::Write`] naming the file that could not be written.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), StoreError> {
    let write_error = |source: std::io::Error| StoreError::Write {
        path: path.to_path_buf(),
        source,
    };

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(write_error)?;
    }

    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);

    fs::write(&temp, contents).map_err(|source| StoreError::Write {
        path: temp.clone(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600)).map_err(|source| {
            StoreError::Write {
                path: temp.clone(),
                source,
            }
        })?;
    }

    fs::rename(&temp, path).map_err(write_error)
}

/// Reads a file, naming it in the error.
///
/// # Errors
///
/// [`StoreError::Read`] naming the file that could not be read.
pub(crate) fn read_to_string(path: &Path) -> Result<String, StoreError> {
    fs::read_to_string(path).map_err(|source| StoreError::Read {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("file.json");
        write_atomic(&path, "{}\n").unwrap();
        assert_eq!(read_to_string(&path).unwrap(), "{}\n");
    }

    #[test]
    fn no_temporary_file_survives_a_successful_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.json");
        write_atomic(&path, "{}\n").unwrap();

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "found {leftovers:?}");
    }

    #[test]
    fn rewriting_replaces_the_previous_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.json");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(read_to_string(&path).unwrap(), "second");
    }

    #[cfg(unix)]
    #[test]
    fn written_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.json");
        write_atomic(&path, "secret").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.json");
        let err = read_to_string(&path).unwrap_err();
        assert!(err.to_string().contains("absent.json"));
    }
}
