/// Apply text-field borders without changing other controls or field geometry.
pub(super) fn bordered(edit: egui::TextEdit<'_>) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| {
        ui.scope(|ui| {
            let visuals = ui.visuals_mut();
            let color = if visuals.dark_mode {
                egui::Color32::from_gray(100)
            } else {
                egui::Color32::from_gray(160)
            };
            visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, color);
            visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, visuals.text_color());
            ui.add(edit)
        })
        .inner
    }
}
