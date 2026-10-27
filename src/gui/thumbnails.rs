//! Mod thumbnails: downloaded outside the UI frame, cached on disk and uploaded on demand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tracing::debug;

/// Largest response accepted from the image host.
const MAX_DOWNLOAD_BYTES: usize = 4 * 1024 * 1024;
/// Disk cache budget. Older files are removed at startup when it is exceeded.
pub(super) const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PARALLEL_LOADS: usize = 4;
/// Largest source image accepted. Downloads are small, but an image header can claim huge
/// dimensions. These limits stop such images before the decoder allocates memory for them.
const MAX_SOURCE_SIDE: u32 = 4096;
const MAX_DECODE_BYTES: u64 = 64 * 1024 * 1024;
/// Large textures are only used by the details panel. Keep a few recent ones.
const MAX_LARGE_TEXTURES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ThumbnailSize {
    /// Row thumbnails, decoded at 2x their largest display size.
    Small,
    /// Details panel image.
    Large,
}

impl ThumbnailSize {
    fn pixels(self) -> (u32, u32) {
        match self {
            ThumbnailSize::Small => (128, 72),
            ThumbnailSize::Large => (320, 180),
        }
    }
}

type Key = (String, ThumbnailSize);
type Loaded = (Key, Result<egui::ColorImage, String>);

enum Entry {
    Loading,
    Ready {
        texture: egui::TextureHandle,
        last_used: u64,
    },
    Failed,
}

pub(super) struct Thumbnails {
    dir: PathBuf,
    entries: HashMap<Key, Entry>,
    tx: Sender<Loaded>,
    rx: Receiver<Loaded>,
    frame: u64,
    permits: Arc<tokio::sync::Semaphore>,
    client: Option<reqwest::Client>,
}

impl Thumbnails {
    pub(super) fn new(dir: PathBuf) -> Self {
        let (tx, rx) = channel();
        Self {
            dir,
            entries: Default::default(),
            tx,
            rx,
            frame: 0,
            permits: Arc::new(tokio::sync::Semaphore::new(MAX_PARALLEL_LOADS)),
            client: None,
        }
    }

    /// Applies finished loads. Call once per frame before rendering rows.
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        while let Ok((key, result)) = self.rx.try_recv() {
            let entry = match result {
                Ok(image) => Entry::Ready {
                    texture: ctx.load_texture(
                        format!("thumbnail-{:?}-{}", key.1, key.0),
                        image,
                        egui::TextureOptions::LINEAR,
                    ),
                    last_used: self.frame,
                },
                Err(error) => {
                    debug!("thumbnail unavailable: {error}");
                    Entry::Failed
                }
            };
            self.entries.insert(key, entry);
        }
        let mut large: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(key, entry)| match entry {
                Entry::Ready { last_used, .. } if key.1 == ThumbnailSize::Large => {
                    Some((*last_used, key.clone()))
                }
                _ => None,
            })
            .collect();
        if large.len() > MAX_LARGE_TEXTURES {
            large.sort_by_key(|(last_used, _)| *last_used);
            for (_, key) in &large[..large.len() - MAX_LARGE_TEXTURES] {
                self.entries.remove(key);
            }
        }
    }

    /// Returns the texture when loaded and starts loading it otherwise. Only `https` URLs load.
    pub(super) fn get(
        &mut self,
        ctx: &egui::Context,
        url: &str,
        size: ThumbnailSize,
    ) -> Option<egui::TextureHandle> {
        let key = (url.to_owned(), size);
        match self.entries.get_mut(&key) {
            Some(Entry::Ready { texture, last_used }) => {
                *last_used = self.frame;
                return Some(texture.clone());
            }
            Some(Entry::Loading | Entry::Failed) => return None,
            None => {}
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.entries.insert(key, Entry::Failed);
            return None;
        };
        if !url.starts_with("https://") {
            self.entries.insert(key, Entry::Failed);
            return None;
        }
        self.entries.insert(key.clone(), Entry::Loading);
        let client = self
            .client
            .get_or_insert_with(|| {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(20))
                    .build()
                    .unwrap_or_default()
            })
            .clone();
        let dir = self.dir.clone();
        let tx = self.tx.clone();
        let permits = self.permits.clone();
        let ctx = ctx.clone();
        runtime.spawn(async move {
            let _permit = permits.acquire_owned().await;
            let result = load(&dir, &key.0, key.1, &client).await;
            let _ = tx.send((key, result));
            ctx.request_repaint();
        });
        None
    }
}

async fn load(
    dir: &Path,
    url: &str,
    size: ThumbnailSize,
    client: &reqwest::Client,
) -> Result<egui::ColorImage, String> {
    let path = cache_path(dir, url);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        match decode_off_thread(bytes, size).await {
            Ok((_, image)) => return Ok(image),
            // A damaged cache file would otherwise fail on every start, so fetch it again.
            Err(error) => {
                debug!("cached thumbnail unreadable, downloading again: {error}");
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }
    let bytes = download(client, url).await?;
    // Decode before caching so a broken response is not reused.
    let (bytes, image) = decode_off_thread(bytes, size).await?;
    if let Err(error) = write_cache(dir, &path, &bytes).await {
        debug!("thumbnail cache write failed: {error}");
    }
    Ok(image)
}

/// Decodes on a blocking thread so large images never stall the async runtime.
async fn decode_off_thread(
    bytes: Vec<u8>,
    size: ThumbnailSize,
) -> Result<(Vec<u8>, egui::ColorImage), String> {
    tokio::task::spawn_blocking(move || {
        let image = decode(&bytes, size)?;
        Ok((bytes, image))
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|error| error.without_url().to_string())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.without_url().to_string())?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_DOWNLOAD_BYTES {
            return Err("thumbnail is too large".into());
        }
    }
    Ok(bytes)
}

async fn write_cache(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = dir.to_owned();
    let path = path.to_owned();
    let bytes = bytes.to_owned();
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        std::fs::create_dir_all(&dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(&bytes)?;
        file.persist(&path).map_err(|error| error.error)?;
        Ok(())
    })
    .await?
}

pub(super) fn cache_path(dir: &Path, url: &str) -> PathBuf {
    dir.join(hex::encode(Sha256::digest(url.as_bytes())))
}

/// Decodes and shrinks an image to fit `size`, keeping its aspect ratio.
pub(super) fn decode(bytes: &[u8], size: ThumbnailSize) -> Result<egui::ColorImage, String> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_SIDE);
    limits.max_image_height = Some(MAX_SOURCE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    reader.limits(limits);
    let image = reader.decode().map_err(|error| error.to_string())?;
    let (width, height) = size.pixels();
    let image = if image.width() > width || image.height() > height {
        image.thumbnail(width, height)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_flat_samples().as_slice(),
    ))
}

/// Removes the oldest cached thumbnails until the directory fits within `max_bytes`.
pub(super) fn prune(dir: &Path, max_bytes: u64) -> std::io::Result<()> {
    let mut files = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            files.push((metadata.modified()?, metadata.len(), entry.path()));
        }
    }
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= max_bytes {
            break;
        }
        std::fs::remove_file(path)?;
        total -= len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn decode_shrinks_to_fit_and_keeps_small_images() {
        let large = decode(&png(640, 360), ThumbnailSize::Small).unwrap();
        assert_eq!(large.size, [128, 72]);
        let tall = decode(&png(100, 400), ThumbnailSize::Large).unwrap();
        assert_eq!(tall.size[1], 180);
        assert!(tall.size[0] < 50);
        let small = decode(&png(20, 10), ThumbnailSize::Large).unwrap();
        assert_eq!(small.size, [20, 10]);
        assert!(decode(b"not an image", ThumbnailSize::Small).is_err());
    }

    #[test]
    fn images_claiming_huge_dimensions_are_rejected() {
        assert!(decode(&png(MAX_SOURCE_SIDE, 2), ThumbnailSize::Small).is_ok());
        assert!(decode(&png(MAX_SOURCE_SIDE + 1, 2), ThumbnailSize::Small).is_err());
        assert!(decode(&png(2, MAX_SOURCE_SIDE + 1), ThumbnailSize::Large).is_err());
    }

    #[tokio::test]
    async fn damaged_cache_files_are_removed_so_they_load_again() {
        let dir = tempfile::tempdir().unwrap();
        // `.invalid` never resolves, so the download fails without reaching any host.
        let url = "https://thumb.example.invalid/broken.png";
        let path = cache_path(dir.path(), url);
        std::fs::write(&path, b"not an image").unwrap();
        let client = reqwest::Client::new();
        assert!(
            load(dir.path(), url, ThumbnailSize::Small, &client)
                .await
                .is_err()
        );
        assert!(
            !path.exists(),
            "the damaged file does not block a later download"
        );
    }

    #[test]
    fn jpeg_thumbnails_decode() {
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(32, 18, image::Rgb([200, 10, 10]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        assert_eq!(decode(&bytes, ThumbnailSize::Small).unwrap().size, [32, 18]);
    }

    #[test]
    fn cache_paths_are_stable_and_distinct() {
        let dir = Path::new("cache");
        assert_eq!(
            cache_path(dir, "https://a/1"),
            cache_path(dir, "https://a/1")
        );
        assert_ne!(
            cache_path(dir, "https://a/1"),
            cache_path(dir, "https://a/2")
        );
        assert_eq!(cache_path(dir, "https://a/1").parent(), Some(dir));
    }

    #[tokio::test]
    async fn cached_files_load_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://thumb.example.invalid/logo.png";
        std::fs::write(cache_path(dir.path(), url), png(320, 180)).unwrap();
        let client = reqwest::Client::new();
        let image = load(dir.path(), url, ThumbnailSize::Small, &client)
            .await
            .unwrap();
        assert_eq!(image.size, [128, 72]);
    }

    #[test]
    fn prune_removes_oldest_files_first() {
        let dir = tempfile::tempdir().unwrap();
        let now = std::time::SystemTime::now();
        for (index, name) in ["old", "middle", "new"].iter().enumerate() {
            let path = dir.path().join(name);
            std::fs::write(&path, [0; 10]).unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(now - Duration::from_secs(100 - index as u64 * 10))
                .unwrap();
        }
        prune(dir.path(), 20).unwrap();
        assert!(!dir.path().join("old").exists());
        assert!(dir.path().join("middle").exists() && dir.path().join("new").exists());
        prune(&dir.path().join("missing"), 0).unwrap();
    }

    #[test]
    fn non_https_and_runtime_less_requests_never_load() {
        let context = egui::Context::default();
        let mut thumbnails = Thumbnails::new(PathBuf::from("unused"));
        assert!(
            thumbnails
                .get(
                    &context,
                    "https://example.invalid/a.png",
                    ThumbnailSize::Small
                )
                .is_none()
        );
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();
        assert!(
            thumbnails
                .get(&context, "file:///etc/passwd", ThumbnailSize::Small)
                .is_none()
        );
        assert!(matches!(
            thumbnails
                .entries
                .get(&("file:///etc/passwd".to_owned(), ThumbnailSize::Small)),
            Some(Entry::Failed)
        ));
    }
}
