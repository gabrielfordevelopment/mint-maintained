use crate::state::GroupColor;
use egui::{Color32, Stroke};
use strum::IntoEnumIterator;

pub(super) fn palette(color: GroupColor, dark: bool) -> (&'static str, Color32) {
    use GroupColor::*;
    let (name, light, dark_color) = match color {
        Gray => ("Gray", [208, 213, 220], [57, 62, 70]),
        Rose => ("Rose", [239, 199, 210], [76, 44, 54]),
        Red => ("Red", [240, 199, 195], [77, 44, 42]),
        Orange => ("Orange", [241, 211, 182], [76, 54, 35]),
        Amber => ("Amber", [238, 222, 173], [70, 60, 32]),
        Lime => ("Lime", [219, 228, 179], [56, 65, 35]),
        Green => ("Green", [190, 225, 198], [33, 65, 44]),
        Teal => ("Teal", [183, 224, 215], [29, 64, 58]),
        Cyan => ("Cyan", [185, 225, 232], [30, 62, 69]),
        Sky => ("Sky", [192, 218, 242], [34, 55, 77]),
        Blue => ("Blue", [199, 209, 243], [40, 47, 79]),
        Indigo => ("Indigo", [209, 205, 240], [48, 43, 77]),
        Violet => ("Violet", [219, 203, 239], [58, 41, 75]),
        Purple => ("Purple", [233, 201, 235], [69, 39, 71]),
        Pink => ("Pink", [241, 200, 224], [77, 40, 63]),
    };
    let [r, g, b] = if dark { dark_color } else { light };
    (name, Color32::from_rgb(r, g, b))
}

pub(super) fn picker(ui: &mut egui::Ui, selected: GroupColor) -> Option<GroupColor> {
    ui.label("Group color");
    let mut choice = None;
    egui::Grid::new("group-color-palette")
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            for (index, color) in GroupColor::iter().enumerate() {
                let (name, fill) = palette(color, ui.visuals().dark_mode);
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        selected == color,
                        name,
                    )
                });
                ui.painter().circle_filled(rect.center(), 9.0, fill);
                if selected == color || response.hovered() || response.has_focus() {
                    ui.painter().circle_stroke(
                        rect.center(),
                        11.0,
                        Stroke::new(1.5_f32, ui.visuals().text_color()),
                    );
                }
                if selected == color {
                    let c = rect.center();
                    ui.painter().add(egui::Shape::line(
                        vec![
                            c + egui::vec2(-4.0, 0.0),
                            c + egui::vec2(-1.0, 3.0),
                            c + egui::vec2(4.0, -3.0),
                        ],
                        Stroke::new(1.5_f32, ui.visuals().text_color()),
                    ));
                }
                if response.clicked() {
                    choice = Some(color);
                    ui.close_menu();
                }
                response.on_hover_text(name);
                if index % 5 == 4 {
                    ui.end_row();
                }
            }
        });
    choice
}
