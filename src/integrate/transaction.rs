use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::IntegrationError;

pub(super) fn check_cancelled(cancelled: &AtomicBool) -> Result<(), IntegrationError> {
    if cancelled.load(Ordering::Acquire) {
        Err(IntegrationError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn lock_installation(directory: &Path) -> Result<std::fs::File, IntegrationError> {
    let path = directory.join(".mint-install.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    lock.try_lock().map_err(|error| IntegrationError::GenericError {
        msg: format!("Cannot lock {} for installation. Another MINT operation may still be running: {error}", path.display()),
    })?;
    Ok(lock)
}

pub(super) struct StagedFile {
    pub file: tempfile::NamedTempFile,
    pub destination: PathBuf,
}

impl StagedFile {
    pub fn new(destination: PathBuf) -> Result<Self, IntegrationError> {
        let parent = destination
            .parent()
            .ok_or_else(|| io::Error::other("Missing output directory"))?;
        Ok(Self {
            file: tempfile::NamedTempFile::new_in(parent)?,
            destination,
        })
    }
}

pub(super) fn commit_files(
    files: Vec<StagedFile>,
    cancelled: &AtomicBool,
) -> Result<(), IntegrationError> {
    commit_files_with(files, cancelled, |file, path| file.persist(path))
}

fn commit_files_with(
    files: Vec<StagedFile>,
    cancelled: &AtomicBool,
    mut replace: impl FnMut(
        tempfile::NamedTempFile,
        &Path,
    ) -> Result<std::fs::File, tempfile::PersistError>,
) -> Result<(), IntegrationError> {
    let mut backups = Vec::new();
    for staged in &files {
        staged.file.as_file().sync_all()?;
        let backup = match fs_err::File::open(&staged.destination) {
            Ok(mut original) => {
                if !original.metadata()?.is_file() {
                    return Err(
                        io::Error::other("Installation target is not a regular file").into(),
                    );
                }
                let mut backup =
                    tempfile::NamedTempFile::new_in(staged.destination.parent().unwrap())?;
                io::copy(&mut original, &mut backup)?;
                backup.flush()?;
                backup.as_file().sync_all()?;
                Some(backup)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        backups.push((staged.destination.clone(), backup));
    }
    check_cancelled(cancelled)?;
    for (index, staged) in files.into_iter().enumerate() {
        if let Err(error) = replace(staged.file, &staged.destination) {
            let mut failures = Vec::new();
            for (destination, backup) in backups.drain(..index).rev() {
                match backup {
                    Some(backup) => {
                        if let Err(restore) = backup.persist(&destination) {
                            let reason = restore.error.to_string();
                            let recovery = restore
                                .file
                                .into_temp_path()
                                .keep()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|error| error.to_string());
                            failures.push(format!(
                                "Could not restore {}: {reason}. Recovery copy: {recovery}",
                                destination.display()
                            ));
                        }
                    }
                    None => {
                        if let Err(error) = fs_err::remove_file(&destination) {
                            failures.push(error.to_string());
                        }
                    }
                }
            }
            return Err(IntegrationError::GenericError {
                msg: format!(
                    "Could not replace {}: {}. {}",
                    staged.destination.display(),
                    error.error,
                    if failures.is_empty() {
                        "The previous installation was preserved.".to_owned()
                    } else {
                        failures.join("\n")
                    }
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn staged(path: &Path, data: &[u8]) -> StagedFile {
        let mut output = StagedFile::new(path.to_owned()).unwrap();
        output.file.write_all(data).unwrap();
        output
    }

    #[test]
    fn successful_commit_replaces_equal_size_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let pak = dir.path().join("mods_P.pak");
        let hook = dir.path().join("hook.dll");
        std::fs::write(&pak, b"old pak").unwrap();
        std::fs::write(&hook, b"old dll").unwrap();
        commit_files(
            vec![staged(&pak, b"new pak"), staged(&hook, b"new dll")],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(std::fs::read(pak).unwrap(), b"new pak");
        assert_eq!(std::fs::read(hook).unwrap(), b"new dll");
    }

    #[test]
    fn failed_second_replace_restores_existing_or_absent_bundle() {
        for existing in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let pak = dir.path().join("mods_P.pak");
            let hook = dir.path().join("hook.dll");
            if existing {
                std::fs::write(&pak, b"working pak").unwrap();
            }
            std::fs::write(&hook, b"working hook").unwrap();
            let result = commit_files_with(
                vec![staged(&pak, b"replacement"), staged(&hook, b"replacement")],
                &AtomicBool::new(false),
                |file, path| {
                    if path == hook {
                        Err(tempfile::PersistError {
                            error: io::Error::new(
                                io::ErrorKind::PermissionDenied,
                                "fixture locked hook",
                            ),
                            file,
                        })
                    } else {
                        file.persist(path)
                    }
                },
            );
            assert!(result.is_err());
            assert_eq!(pak.exists(), existing);
            if existing {
                assert_eq!(std::fs::read(&pak).unwrap(), b"working pak");
            }
            assert_eq!(std::fs::read(&hook).unwrap(), b"working hook");
            commit_files(
                vec![staged(&pak, b"retry pak"), staged(&hook, b"retry hook")],
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(std::fs::read(&pak).unwrap(), b"retry pak");
        }
    }

    #[test]
    fn cancellation_before_commit_preserves_all_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let pak = dir.path().join("mods_P.pak");
        std::fs::write(&pak, b"working pak").unwrap();
        let staged = staged(&pak, b"replacement");
        assert!(matches!(
            commit_files(vec![staged], &AtomicBool::new(true)),
            Err(IntegrationError::Cancelled)
        ));
        assert_eq!(std::fs::read(pak).unwrap(), b"working pak");
    }

    #[test]
    fn installation_lock_blocks_concurrent_writers_and_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let first = lock_installation(dir.path()).unwrap();
        assert!(lock_installation(dir.path()).is_err());
        drop(first);
        assert!(lock_installation(dir.path()).is_ok());
    }
}
