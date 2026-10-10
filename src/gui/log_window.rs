use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncSeekExt};

const MAX_LOG_BYTES: u64 = 128 * 1024;

fn with_local_times(text: &str) -> String {
    annotate_times(text, |timestamp| {
        timestamp.with_timezone(&chrono::Local).fixed_offset()
    })
}

fn annotate_times(
    text: &str,
    local: impl Fn(chrono::DateTime<chrono::FixedOffset>) -> chrono::DateTime<chrono::FixedOffset>,
) -> String {
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let timestamp = line
            .split_once(' ')
            .map_or(line.trim_end(), |(timestamp, _)| timestamp);
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            output.push_str(timestamp);
            output.push_str(&format!(
                " [{} local]",
                local(parsed).format("%Y-%m-%d %H:%M:%S %:z")
            ));
            output.push_str(&line[timestamp.len()..]);
        } else {
            output.push_str(line);
        }
    }
    output
}

pub(super) struct LogWindow {
    updates: tokio::sync::watch::Receiver<Result<String, String>>,
    worker: tokio::task::JoinHandle<()>,
    text: String,
    error: Option<String>,
    paused: bool,
    follow: bool,
    on_top: bool,
}

impl Drop for LogWindow {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

async fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let length = file.metadata().await?.len();
    let start = length.saturating_sub(MAX_LOG_BYTES);
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut bytes = Vec::new();
    file.take(MAX_LOG_BYTES).read_to_end(&mut bytes).await?;
    if start > 0 {
        let skip = bytes.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1);
        bytes.drain(..skip);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

impl LogWindow {
    pub fn new(ctx: egui::Context, path: PathBuf, secrets: Vec<String>) -> Self {
        let (tx, updates) = tokio::sync::watch::channel(Ok(String::new()));
        let worker = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            loop {
                interval.tick().await;
                let update = read_tail(&path)
                    .await
                    .map(|text| {
                        with_local_times(&super::diagnostics::redact(
                            &text,
                            secrets.iter().cloned(),
                        ))
                    })
                    .map_err(|error| format!("Could not read the log: {error}"));
                if *tx.borrow() != update {
                    if tx.send(update).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Self {
            updates,
            worker,
            text: String::new(),
            error: None,
            paused: false,
            follow: true,
            on_top: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("live-log"),
            egui::ViewportBuilder::default()
                .with_title("MINT Maintained — Live log")
                .with_inner_size([800.0, 420.0]),
            |ctx, class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    open = false;
                }
                if class == egui::ViewportClass::Embedded {
                    egui::Window::new("Live log")
                        .open(&mut open)
                        .show(ctx, |ui| self.ui(ui));
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| self.ui(ui));
                }
            },
        );
        open
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.paused && self.updates.has_changed().unwrap_or(false) {
            match self.updates.borrow_and_update().clone() {
                Ok(text) => {
                    self.text = text;
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.follow, "Follow log");
            ui.checkbox(&mut self.paused, "Pause");
            if ui.checkbox(&mut self.on_top, "Always on top").changed() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }));
            }
            if ui.button("Copy log").clicked() {
                ui.ctx().copy_text(self.text.clone());
            }
        });
        ui.separator();
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.text.is_empty() {
            ui.label("Waiting for log entries…");
        }
        let lines: Vec<_> = self.text.lines().collect();
        egui::ScrollArea::both()
            .stick_to_bottom(self.follow)
            .auto_shrink([false, false])
            .show_rows(
                ui,
                ui.text_style_height(&egui::TextStyle::Monospace),
                lines.len(),
                |ui, range| {
                    for line in &lines[range] {
                        ui.add(
                            egui::Label::new(egui::RichText::new(*line).monospace())
                                .selectable(true)
                                .extend(),
                        );
                    }
                },
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_timestamps_preserve_original_entries_and_handle_date_rollover() {
        let text =
            "2026-10-10T23:42:08.123456Z INFO moved mod\n  continuation\ninvalid timestamp\n";
        let annotated = annotate_times(text, |dt| {
            dt.with_timezone(&chrono::FixedOffset::east_opt(7200).unwrap())
        });
        assert_eq!(
            annotated,
            "2026-10-10T23:42:08.123456Z [2026-10-11 01:42:08 +02:00 local] INFO moved mod\n  continuation\ninvalid timestamp\n"
        );
        let winter = annotate_times("2026-12-10T07:42:08Z INFO winter", |dt| {
            dt.with_timezone(&chrono::FixedOffset::east_opt(3600).unwrap())
        });
        assert!(winter.contains("[2026-12-10 08:42:08 +01:00 local]"));
        assert!(!winter.ends_with('\n'));
        assert_eq!(
            with_local_times("plain Unicode ő log\n"),
            "plain Unicode ő log\n"
        );
    }

    #[tokio::test]
    async fn tail_refreshes_after_append_truncation_and_replacement_and_stays_bounded() {
        use tokio::io::AsyncWriteExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mint.log");
        assert!(read_tail(&path).await.is_err());
        tokio::fs::write(&path, "first\n").await.unwrap();
        assert_eq!(read_tail(&path).await.unwrap(), "first\n");
        let mut writer = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .await
            .unwrap();
        writer.write_all(b"second\n").await.unwrap();
        writer.flush().await.unwrap();
        drop(writer);
        assert_eq!(read_tail(&path).await.unwrap(), "first\nsecond\n");
        tokio::fs::write(&path, "reset\n").await.unwrap();
        assert_eq!(read_tail(&path).await.unwrap(), "reset\n");
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::write(
            &path,
            format!("{}last İ line\n", "older line\n".repeat(20_000)),
        )
        .await
        .unwrap();
        let tail = read_tail(&path).await.unwrap();
        assert!(tail.len() <= MAX_LOG_BYTES as usize);
        assert!(tail.ends_with("last İ line\n"));
        assert!(!tail.contains('\u{fffd}'));
    }

    #[tokio::test]
    async fn live_reader_refreshes_redacts_and_stops_when_the_viewer_closes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mint.log");
        tokio::fs::write(&path, "first entry with fixture-secret\n")
            .await
            .unwrap();
        let mut viewer = LogWindow::new(
            egui::Context::default(),
            path.clone(),
            vec!["fixture-secret".into()],
        );
        tokio::time::timeout(Duration::from_secs(3), viewer.updates.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            viewer.updates.borrow_and_update().as_ref().unwrap(),
            "first entry with [redacted]\n"
        );
        tokio::fs::write(&path, "a later entry\n").await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), viewer.updates.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            viewer.updates.borrow_and_update().as_ref().unwrap(),
            "a later entry\n"
        );
        let abort = viewer.worker.abort_handle();
        drop(viewer);
        tokio::time::timeout(Duration::from_secs(3), async {
            while !abort.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
