// Adapted from
// <https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/toggle_switch.rs>.

fn toggle_ui(ui: &mut egui::Ui, on: &mut bool, stable: bool) -> egui::Response {
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
    let (auto_id, rect) = ui.allocate_space(desired_size);
    let id = if stable {
        ui.make_persistent_id("enabled-switch")
    } else {
        auto_id
    };
    let mut response = ui.interact(rect, id, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, "")
    });

    if ui.is_rect_visible(rect) {
        let state_id = response.id.with("last-value");
        let previous = ui.data(|data| data.get_temp::<bool>(state_id));
        let how_on = if stable && previous.is_some_and(|value| value != *on) && !response.changed()
        {
            ui.ctx().animate_bool_with_time(response.id, *on, 0.0)
        } else {
            ui.ctx().animate_bool(response.id, *on)
        };
        ui.data_mut(|data| data.insert_temp(state_id, *on));
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter().rect(
            rect,
            radius,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    response
}

#[allow(clippy::needless_pass_by_ref_mut)]
pub fn toggle_switch(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| toggle_ui(ui, on, false)
}

pub fn stable_toggle_switch(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| toggle_ui(ui, on, true)
}
