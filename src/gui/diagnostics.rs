use std::error::Error;

pub(super) fn error_details(error: &(dyn Error + 'static)) -> String {
    let mut details = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        let message = error.to_string();
        if !details.contains(&message) {
            details.push_str(&format!("\nCaused by: {message}"));
        }
        source = error.source();
    }
    details
}

pub(super) fn redact(text: &str, secrets: impl Iterator<Item = String>) -> String {
    let mut redacted = text.to_owned();
    for secret in secrets.filter(|value| !value.is_empty()) {
        redacted = redacted.replace(&secret, "[redacted]");
    }
    static AUTH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    AUTH.get_or_init(|| {
        regex::Regex::new(r"(?i)(bearer\s+|(?:access_token|token|api_key|oauth)=)[^\s&<>]+")
            .unwrap()
    })
    .replace_all(&redacted, "${1}[redacted]")
    .into_owned()
}

pub(super) fn details_ui(ui: &mut egui::Ui, details: &str) {
    if ui.button("Copy error").clicked() {
        ui.ctx().copy_text(details.to_owned());
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add(egui::Label::new(details).selectable(true).wrap());
    });
}
