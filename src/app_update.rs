//! Application downloads and executable replacement, independent of the GUI.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use futures::TryStreamExt;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Preparing,
    Installing,
}

#[derive(Debug, Default)]
pub struct Control(AtomicU8);

impl Control {
    pub fn cancel(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn can_cancel(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }

    pub fn is_installing(&self) -> bool {
        self.0.load(Ordering::Acquire) == 2
    }

    fn begin_install(&self) -> bool {
        self.0
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    async fn cancelled(&self) {
        while self.0.load(Ordering::Acquire) != 1 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }
}

pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        "mint-x86_64-pc-windows-msvc.zip"
    } else {
        "mint-x86_64-unknown-linux-gnu.zip"
    }
}

pub async fn install(
    url: String,
    control: Arc<Control>,
    progress: impl Fn(Progress) + Send + 'static,
) -> Result<Option<PathBuf>> {
    let executable = std::env::current_exe().context("Could not locate the running application")?;
    let staging = tempfile::Builder::new()
        .prefix(".mint-update-")
        .tempdir_in(
            executable
                .parent()
                .context("Application directory is missing")?,
        )
        .context(
            "Could not create update files beside the application. Check folder permissions",
        )?;
    let archive = staging.path().join("update.zip");
    let downloaded = tokio::select! {
        biased;
        _ = control.cancelled() => return Ok(None),
        result = download(&url, &archive, &progress) => result,
    };
    downloaded?;
    progress(Progress::Preparing);
    tokio::task::spawn_blocking(move || {
        let candidate = extract(&archive, staging.path())?;
        if !control.begin_install() {
            return Ok(None);
        }
        progress(Progress::Installing);
        tracing::info!("Replacing the application executable");
        replace_current_executable(&candidate)?;
        Ok(Some(executable))
    })
    .await
    .context("The update worker stopped unexpectedly")?
}

async fn download(url: &str, destination: &Path, progress: &impl Fn(Progress)) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(mint_lib::update::GITHUB_REQ_USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()?;
    let response = client
        .get(url)
        .send()
        .await
        .context("Could not download the application update")?
        .error_for_status()
        .context("The update server rejected the download")?;
    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut output = tokio::fs::File::create(destination)
        .await
        .context("Could not create the update archive")?;
    let mut downloaded = 0;
    while let Some(bytes) = stream
        .try_next()
        .await
        .context("The update download was interrupted")?
    {
        output
            .write_all(&bytes)
            .await
            .context("Could not write the update archive")?;
        downloaded += bytes.len() as u64;
        progress(Progress::Downloading { downloaded, total });
    }
    anyhow::ensure!(
        total.is_none_or(|total| total == downloaded),
        "The update download is incomplete"
    );
    output
        .flush()
        .await
        .context("Could not finish writing the update archive")?;
    output
        .sync_all()
        .await
        .context("Could not save the update archive")?;
    Ok(())
}

fn extract(archive: &Path, directory: &Path) -> Result<PathBuf> {
    let name = if cfg!(windows) { "mint.exe" } else { "mint" };
    self_update::Extract::from_source(archive)
        .archive(self_update::ArchiveKind::Zip)
        .extract_file(directory, name)
        .context(
            "Could not unpack the application update. The archive may be incomplete or invalid",
        )?;
    let candidate = directory.join(name);
    let mut header = [0; 4];
    use std::io::Read;
    std::fs::File::open(&candidate)?
        .read_exact(&mut header)
        .context("The update executable is incomplete")?;
    anyhow::ensure!(
        if cfg!(windows) {
            &header[..2] == b"MZ"
        } else {
            &header == b"\x7fELF"
        },
        "The update does not contain an executable for this platform"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests;

/// Keep a recoverable copy even if the platform replacement fails after moving the old image.
pub fn replace_current_executable(candidate: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    let backup = tempfile::Builder::new()
        .prefix(".mint-recovery-")
        .tempdir_in(
            executable
                .parent()
                .context("Application directory is missing")?,
        )?;
    let saved = backup
        .path()
        .join(if cfg!(windows) { "mint.exe" } else { "mint" });
    std::fs::copy(&executable, &saved).context("Could not back up the current application")?;
    for attempt in 0..21 {
        match self_update::self_replace::self_replace(candidate) {
            Ok(()) => return Ok(()),
            Err(error) => {
                if !executable.exists()
                    && let Err(recovery) = std::fs::copy(&saved, &executable)
                {
                    let preserved = backup.keep();
                    anyhow::bail!(
                        "Could not replace the application: {error}. Automatic recovery failed: {recovery}. The previous executable is preserved in {}",
                        preserved.display()
                    );
                }
                if cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33)) && attempt < 20 {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                return Err(error).context("Could not replace the application executable. Close other MINT instances and retry, or install the release manually");
            }
        }
    }
    unreachable!("The final attempt always returns")
}
