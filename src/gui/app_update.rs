use super::*;
use crate::app_update::{Control, Progress};
use std::sync::Arc;

impl App {
    pub(super) fn show_update_window(&mut self, ctx: &egui::Context) {
        if !self.show_app_update {
            return;
        }
        let Some(release) = &self.available_update else {
            return;
        };
        let tag = release.tag_name.clone();
        let busy = self.self_update_rid.is_some();
        let completed = self.original_exe_path.is_some();
        let cancellable = self
            .self_update_control
            .as_ref()
            .is_some_and(|c| c.can_cancel());
        let can_install = !self.editing_busy();
        let can_close = !busy || completed || cancellable;
        let mut dismiss = false;
        let mut start = false;
        let mut cancel = false;
        let mut restart = false;
        let mut details = false;
        let failed = self.app_update_status.starts_with("Update failed")
            || self.app_update_status.starts_with("Update stopped");
        let width = 560.0_f32.min((ctx.screen_rect().width() - 48.0).max(160.0));
        let notes_height = (ctx.screen_rect().height() - 240.0).clamp(40.0, 340.0);
        let response = egui::Modal::new(egui::Id::new("application-update")).show(ctx, |ui| {
            ui.set_width(width);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.horizontal(|ui| {
                ui.heading(format!("Update available: {tag}"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    dismiss = ui
                        .add_enabled_ui(can_close, |ui| {
                            icons::button(ui, Icon::Close, "Close update")
                        })
                        .inner
                        .clicked();
                });
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("application-release-notes")
                .max_height(notes_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_max_width(width);
                    CommonMarkViewer::new()
                        .max_image_width(Some(width as usize))
                        .show(ui, &mut self.cache, &release.body);
                });
            ui.separator();
            if let Some(handle) = &self.self_update_rid {
                let progress = if self
                    .self_update_control
                    .as_ref()
                    .is_some_and(|c| c.is_installing())
                {
                    Progress::Installing
                } else {
                    handle.state.clone()
                };
                match &progress {
                    Progress::Downloading { downloaded, total } => {
                        let fraction = total
                            .filter(|total| *total > 0)
                            .map(|total| *downloaded as f32 / total as f32);
                        ui.label("Downloading update...");
                        let bar = egui::ProgressBar::new(fraction.unwrap_or(0.0))
                            .animate(fraction.is_none());
                        ui.add(if fraction.is_some() {
                            bar.show_percentage()
                        } else {
                            bar.text(format!(
                                "{:.1} MiB downloaded",
                                *downloaded as f64 / 1_048_576.0
                            ))
                        });
                    }
                    Progress::Preparing => {
                        ui.label("Checking and unpacking update...");
                    }
                    Progress::Installing => {
                        ui.label("Installing update. Please wait...");
                    }
                }
            } else if !self.app_update_status.is_empty() {
                ui.add(egui::Label::new(&self.app_update_status).wrap());
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if completed {
                    restart = ui.button("Restart now").clicked();
                    dismiss |= ui.button("Later").clicked();
                } else if busy {
                    cancel = ui
                        .add_enabled(cancellable, egui::Button::new("Cancel"))
                        .clicked();
                } else {
                    start = ui
                        .add_enabled(can_install, egui::Button::new("Install update"))
                        .clicked();
                    dismiss |= ui.button("Remind me later").clicked();
                    if failed {
                        details = ui.button("Details").clicked();
                    }
                }
            });
        });
        if can_close
            && (dismiss || response.should_close())
            && (!busy
                || completed
                || self
                    .self_update_control
                    .as_ref()
                    .is_some_and(|c| c.cancel()))
        {
            self.show_app_update = false;
            self.update_dismissed = true;
        }
        if cancel && let Some(control) = &self.self_update_control {
            control.cancel();
        }
        if start {
            let control = Arc::new(Control::default());
            self.self_update_control = Some(control.clone());
            self.self_update_rid = Some(message::SelfUpdate::send(
                &mut self.request_counter,
                self.tx.clone(),
                ctx.clone(),
                &tag,
                control,
            ));
        }
        if restart {
            self.needs_restart = true;
        }
        if details {
            self.show_app_update = false;
            self.update_dismissed = true;
            self.show_error_details = true;
        }
    }
}
