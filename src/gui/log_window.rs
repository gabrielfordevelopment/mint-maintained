use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncSeekExt};

const MAX_LOG_BYTES: u64 = 128 * 1024;

fn format_utc_times(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let timestamp = line
            .split_once(' ')
            .map_or(line.trim_end(), |(timestamp, _)| timestamp);
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            output.push_str(&format!(
                "[{}]",
                parsed
                    .with_timezone(&chrono::Utc)
                    .format("%Y-%m-%d %H:%M:%S")
            ));
            output.push_str(&line[timestamp.len()..]);
        } else {
            output.push_str(line);
        }
    }
    output
}

#[derive(Clone, Copy)]
enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

fn visible_log(
    text: &str,
    show_debug: bool,
    visuals: &egui::Visuals,
    font: egui::FontId,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut level = LogLevel::Info;
    for line in text.split_inclusive('\n') {
        if let Some((timestamp, message)) = line.strip_prefix('[').and_then(|s| s.split_once("] "))
            && chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S").is_ok()
        {
            level = match message.split_whitespace().next() {
                Some("DEBUG" | "TRACE") => LogLevel::Debug,
                Some("WARN") => LogLevel::Warn,
                Some("ERROR") => LogLevel::Error,
                _ => LogLevel::Info,
            };
        }
        if !show_debug && matches!(level, LogLevel::Debug) {
            continue;
        }
        let color = match (level, visuals.dark_mode) {
            (LogLevel::Warn, true) => egui::Color32::from_rgb(235, 190, 110),
            (LogLevel::Warn, false) => egui::Color32::from_rgb(145, 87, 10),
            (LogLevel::Error, true) => egui::Color32::from_rgb(245, 145, 145),
            (LogLevel::Error, false) => egui::Color32::from_rgb(175, 40, 48),
            _ => visuals.text_color(),
        };
        job.append(
            line,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
    }
    job
}

pub(super) struct LogWindow {
    updates: tokio::sync::watch::Receiver<Result<String, String>>,
    worker: tokio::task::JoinHandle<()>,
    text: String,
    error: Option<String>,
    paused: bool,
    follow: bool,
    on_top: bool,
    show_debug: bool,
    rendered: egui::text::LayoutJob,
    rendered_dark: Option<bool>,
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
                        format_utc_times(&super::diagnostics::redact(
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
            show_debug: false,
            rendered: Default::default(),
            rendered_dark: None,
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
        let mut refresh = self.rendered_dark != Some(ui.visuals().dark_mode);
        if !self.paused && self.updates.has_changed().unwrap_or(false) {
            match self.updates.borrow_and_update().clone() {
                Ok(text) => {
                    self.text = text;
                    refresh = true;
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        let mut copy = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Time: UTC");
            ui.separator();
            refresh |= ui.checkbox(&mut self.show_debug, "Show debug").changed();
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
            copy = ui.button("Copy visible log").clicked();
        });
        if refresh {
            self.rendered = visible_log(
                &self.text,
                self.show_debug,
                ui.visuals(),
                egui::TextStyle::Monospace.resolve(ui.style()),
            );
            self.rendered_dark = Some(ui.visuals().dark_mode);
        }
        if copy {
            ui.ctx().copy_text(self.rendered.text.clone());
        }
        ui.separator();
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.text.is_empty() {
            ui.label("Waiting for log entries…");
        } else if self.rendered.text.is_empty() {
            ui.label("No INFO, WARN or ERROR entries. Enable Show debug for more detail.");
        }
        egui::ScrollArea::vertical()
            .stick_to_bottom(self.follow)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(self.rendered.clone())
                        .selectable(true)
                        .wrap(),
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_preserves_severity_continuations_and_unstructured_messages() {
        let text = "unstructured ő message\n[2026-10-10 12:00:00] DEBUG hidden\n  hidden continuation\n[2026-10-10 12:00:01] INFO visible DEBUG word\n  visible continuation\n[2026-10-10 12:00:02] WARN warning\n[2026-10-10 12:00:03] ERROR failure\n  failure details\n[2026-10-10 12:00:04] TRACE hidden trace\n";
        for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
            let font = egui::FontId::monospace(12.0);
            let normal = visible_log(text, false, &visuals, font.clone());
            assert!(!normal.text.contains("hidden"));
            assert!(normal.text.contains("unstructured ő message"));
            assert!(
                normal
                    .text
                    .contains("visible DEBUG word\n  visible continuation")
            );
            assert!(normal.text.contains("failure\n  failure details"));
            let warning = normal
                .sections
                .iter()
                .find(|s| normal.text[s.byte_range.clone()].contains("WARN"))
                .unwrap();
            let error = normal
                .sections
                .iter()
                .find(|s| normal.text[s.byte_range.clone()].contains("ERROR"))
                .unwrap();
            assert_ne!(warning.format.color, error.format.color);
            assert_ne!(error.format.color, visuals.text_color());
            assert_eq!(visible_log(text, true, &visuals, font).text, text);
        }
    }

    #[test]
    fn long_log_messages_wrap_within_the_viewport() {
        let context = egui::Context::default();
        let _ = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 300.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let text = format!(
                        "[2026-10-10 12:00:00] WARN {}",
                        "A long Unicode ő message. ".repeat(20)
                    );
                    let job =
                        visible_log(&text, false, ui.visuals(), egui::FontId::monospace(12.0));
                    let available = ui.available_width();
                    let response = ui.add(egui::Label::new(job).wrap().selectable(true));
                    assert!(response.rect.width() <= available);
                    assert!(response.rect.height() > 24.0);
                });
            },
        );
    }

    #[test]
    fn utc_timestamps_are_compact_and_preserve_messages_and_continuations() {
        let text =
            "2026-10-10T23:42:08.123456Z INFO moved mod\n  continuation\ninvalid timestamp\n";
        assert_eq!(
            format_utc_times(text),
            "[2026-10-10 23:42:08] INFO moved mod\n  continuation\ninvalid timestamp\n"
        );
        assert_eq!(
            format_utc_times("2026-10-11T01:42:08+02:00 INFO entry"),
            "[2026-10-10 23:42:08] INFO entry"
        );
        assert_eq!(
            format_utc_times("plain Unicode ő log\n"),
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
