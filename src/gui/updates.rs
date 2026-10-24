use super::*;
use crate::providers::refresh::RefreshReport;

pub(super) fn redact_report(report: &mut RefreshReport, secrets: &[String]) {
    let redact = |text: &str| diagnostics::redact(text, secrets.iter().cloned());
    for failure in &mut report.failures {
        failure.name = redact(&failure.name);
        failure.details = redact(&failure.details);
    }
    for name in &mut report.new_versions {
        *name = redact(name);
    }
    report.save_error = report.save_error.as_deref().map(redact);
}

pub(super) fn report_ui(
    ui: &mut egui::Ui,
    report: &RefreshReport,
    enabled: bool,
) -> Option<Vec<ModSpecification>> {
    ui.label(report.summary());
    let mut retry = None;
    ui.horizontal(|ui| {
        if ui.button("Copy details").clicked() {
            ui.ctx().copy_text(report.details());
        }
        if (!report.failures.is_empty()
            || !report.remaining.is_empty()
            || report.save_error.is_some())
            && ui
                .add_enabled(
                    enabled,
                    egui::Button::new(if report.cancelled {
                        "Resume remaining"
                    } else {
                        "Retry failed"
                    }),
                )
                .clicked()
        {
            retry = Some(report.retry_specs());
        }
    });
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .show(ui, |ui| {
            if !report.new_versions.is_empty() {
                icons::collapsing_header("New versions found").default_open(true).show(ui, |ui| {
                    for name in &report.new_versions { ui.add(egui::Label::new(name).wrap()); }
                    ui.label("Mods set to latest will use the new version on the next installation. Pinned versions stay unchanged.");
                });
            }
            if let Some(error) = &report.save_error {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "The refreshed cache could not be saved.",
                );
                icons::collapsing_header("Cache write details").show(ui, |ui| {
                    ui.add(egui::Label::new(error).wrap().selectable(true));
                });
            }
            for (index, failure) in report.failures.iter().enumerate() {
                ui.push_id(index, |ui| {
                    ui.separator();
                    ui.add(egui::Label::new(egui::RichText::new(&failure.name).strong()).wrap());
                    ui.add(egui::Label::new(failure.reason).wrap());
                    icons::collapsing_header("Technical details").show(ui, |ui| {
                        ui.add(
                            egui::Label::new(&failure.details)
                                .wrap()
                                .selectable(true),
                        );
                    });
                });
            }
        });
    retry
}

impl App {
    pub(super) fn profile_update_specs(&self) -> Vec<ModSpecification> {
        let mut specs = Vec::new();
        self.state
            .mod_data
            .for_each_mod(&self.state.mod_data.active_profile, |mc| {
                if mc.spec.url.starts_with("https://mod.io/") {
                    specs.push(mc.spec.clone());
                }
            });
        specs
    }

    pub(super) fn status_row(&mut self, ui: &mut Ui) {
        let row_height = ui.spacing().interact_size.y;
        let size = egui::vec2(ui.available_width(), row_height);
        ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
            if !self.installation_status.summary.is_empty() {
                let width = (ui.available_width() * 0.48).min(440.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, row_height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        icons::show(ui, Icon::Update);
                        ui.add(egui::Label::new(&self.installation_status.summary).truncate())
                            .on_hover_text(&self.installation_status.summary);
                    },
                );
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if self.update_rid.is_some() {
                    let remaining = crate::providers::modio::rate_limit::shared().remaining();
                    ui.spinner();
                    let done = self
                        .refresh_progress
                        .load(std::sync::atomic::Ordering::Acquire);
                    let text = if !remaining.is_zero() {
                        format!(
                            "mod.io requests paused. Retrying in {} seconds. ({done}/{})",
                            remaining.as_secs() + 1,
                            self.refresh_total
                        )
                    } else {
                        format!("Checking mod updates: {done}/{}", self.refresh_total)
                    };
                    ui.add(egui::Label::new(text).truncate());
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                } else if let Some(action) = &self.last_action {
                    let (label, color, text) = match &action.status {
                        LastActionStatus::Success(text) => {
                            ("Done", Color32::from_rgb(42, 112, 66), text)
                        }
                        LastActionStatus::Failure(text) => {
                            ("Failed", Color32::from_rgb(160, 43, 48), text)
                        }
                    };
                    egui::Frame::NONE
                        .fill(color)
                        .corner_radius(3)
                        .inner_margin(egui::Margin::symmetric(5, 2))
                        .show(ui, |ui| {
                            ui.label(RichText::new(label).color(Color32::WHITE));
                        });
                    if (action.report.is_some()
                        || matches!(action.status, LastActionStatus::Failure(_)))
                        && ui.button("Details").clicked()
                    {
                        self.show_error_details = true;
                    }
                    ui.add(
                        egui::Label::new(format!(
                            "{} ({})",
                            text.lines().next().unwrap_or_default(),
                            action.timeago()
                        ))
                        .truncate(),
                    );
                    ui.ctx().request_repaint_after(Duration::from_secs(1));
                } else if self.state.config.drg_pak_path.is_none() {
                    ui.weak("Select your DRG installation in Settings to install mods.");
                } else {
                    ui.weak("Ready");
                }
            });
        });
    }
}
