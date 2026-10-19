use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

const CLEANUP_SCRIPT: &str = include_str!("cleanup.ps1");

pub fn replace_current_executable(candidate: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    let directory = tempfile::Builder::new()
        .prefix(".mint-replace-")
        .tempdir_in(
            executable
                .parent()
                .context("Application directory is missing")?,
        )?;
    let next = directory.path().join("next.exe");
    fs::copy(candidate, &next).context("Could not stage the new application")?;
    fs::OpenOptions::new().write(true).open(&next)?.sync_all()?;

    let cleanup = Cleanup::start(directory.path())?;
    install_staged(&executable, directory, cleanup)
}

fn install_staged(
    executable: &Path,
    directory: tempfile::TempDir,
    mut cleanup: Cleanup,
) -> Result<()> {
    let next = directory.path().join("next.exe");
    let previous = directory.path().join("previous.exe");
    retry_sharing_violation(|| fs::rename(executable, &previous))
        .context("Could not replace the application executable. Close other MINT instances and retry, or install the release manually")?;

    let moved = retry_sharing_violation(|| fs::rename(&next, executable));
    let new_installed = moved.is_ok();
    let installed = moved.and_then(|()| {
        retry_sharing_violation(|| {
            fs::rename(
                directory.path().join("ready"),
                directory.path().join("committed"),
            )
        })
    });
    if let Err(error) = installed {
        cleanup.stop();
        let recovered = (|| {
            if new_installed {
                retry_sharing_violation(|| fs::rename(executable, &next))?;
            }
            retry_sharing_violation(|| fs::rename(&previous, executable))
        })();
        if let Err(recovery) = recovered {
            let preserved = directory.keep();
            anyhow::bail!(
                "Could not replace the application: {error}. Automatic recovery failed: {recovery}. The previous executable is preserved in {}",
                preserved.join("previous.exe").display()
            );
        }
        return Err(error)
            .context("Could not install the update; the previous application was restored");
    }

    let _ = directory.keep();
    cleanup.detach();
    Ok(())
}

fn retry_sharing_violation(
    mut operation: impl FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    for attempt in 0..21 {
        match operation() {
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) && attempt < 20 => {
                std::thread::sleep(Duration::from_millis(500));
            }
            result => return result,
        }
    }
    unreachable!("The final attempt always returns")
}

struct Cleanup(Option<Child>);

impl Cleanup {
    fn start(directory: &Path) -> Result<Self> {
        Self::start_with_program(directory, &powershell()?, std::process::id())
    }

    fn start_with_program(directory: &Path, program: &Path, parent_id: u32) -> Result<Self> {
        let child = Command::new(program)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                CLEANUP_SCRIPT,
            ])
            .env("MINT_UPDATE_DIRECTORY", directory)
            .env("MINT_UPDATE_PARENT", parent_id.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .context("Could not start Windows update cleanup; the application was not changed")?;
        let mut cleanup = Self(Some(child));
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = cleanup.0.as_mut().unwrap().try_wait()? {
                anyhow::bail!(
                    "Windows update cleanup stopped before it was ready ({status}); the application was not changed"
                );
            }
            if directory.join("ready").is_file() {
                return Ok(cleanup);
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Windows update cleanup did not become ready; the application was not changed"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn detach(&mut self) {
        self.0.take();
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.stop();
    }
}

fn powershell() -> Result<PathBuf> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }
    let mut buffer = [0_u16; 32768];
    // The system directory avoids executable lookup through the working directory or PATH.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    anyhow::ensure!(
        length > 0 && length < buffer.len(),
        "Could not locate Windows PowerShell"
    );
    use std::os::windows::ffi::OsStringExt;
    Ok(
        PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]))
            .join("WindowsPowerShell/v1.0/powershell.exe"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_cleanup_leaves_staged_files_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let next = directory.path().join("next.exe");
        fs::write(&next, b"staged executable").unwrap();
        assert!(
            Cleanup::start_with_program(
                directory.path(),
                &directory.path().join("missing.exe"),
                std::process::id()
            )
            .is_err()
        );
        assert_eq!(fs::read(next).unwrap(), b"staged executable");
    }

    #[test]
    fn failed_installation_and_commit_restore_the_previous_executable() {
        for missing in ["next.exe", "ready"] {
            let root = tempfile::tempdir().unwrap();
            let executable = root.path().join("mint.exe");
            fs::write(&executable, b"original").unwrap();
            let directory = tempfile::tempdir_in(root.path()).unwrap();
            fs::write(directory.path().join("next.exe"), b"updated").unwrap();
            let cleanup = Cleanup::start(directory.path()).unwrap();
            fs::remove_file(directory.path().join(missing)).unwrap();
            assert!(install_staged(&executable, directory, cleanup).is_err());
            assert_eq!(fs::read(executable).unwrap(), b"original");
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn cleanup_parent() {
        let Some(directory) = std::env::var_os("MINT_CLEANUP_PARENT") else {
            return;
        };
        let released = PathBuf::from(directory).join("exit");
        let deadline = Instant::now() + Duration::from_secs(30);
        while !released.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn cleanup_waits_for_exit_and_preserves_uncommitted_recovery() {
        for committed in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let directory = root.path().join("update");
            fs::create_dir(&directory).unwrap();
            let previous = directory.join("previous.exe");
            fs::write(&previous, b"previous executable").unwrap();
            let mut parent = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app_update::windows::tests::cleanup_parent"])
                .env("MINT_CLEANUP_PARENT", root.path())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000)
                .spawn()
                .unwrap();
            let mut cleanup =
                Cleanup::start_with_program(&directory, &powershell().unwrap(), parent.id())
                    .unwrap();
            if committed {
                fs::rename(directory.join("ready"), directory.join("committed")).unwrap();
            }
            assert_eq!(fs::read(&previous).unwrap(), b"previous executable");
            assert!(cleanup.0.as_mut().unwrap().try_wait().unwrap().is_none());
            fs::write(root.path().join("exit"), "").unwrap();
            assert!(parent.wait().unwrap().success());
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = cleanup.0.as_mut().unwrap().try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(25));
            }
            if committed {
                assert!(!directory.exists());
            } else {
                assert_eq!(fs::read(&previous).unwrap(), b"previous executable");
                assert!(directory.join("ready").exists());
            }
        }
    }
}
