use super::*;

#[test]
fn disclosure_chevron_rotates_continuously_and_reuses_its_texture() {
    for dark in [false, true] {
        let context = egui::Context::default();
        context.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        context.style_mut(|style| style.animation_time = 0.2);
        let mut time = 0.0;
        let mut frame = |expanded| {
            time += 1.0 / 60.0;
            let mut openness = 0.0;
            let output = context.run(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        openness = icons::collapsing_header("Details")
                            .open(Some(expanded))
                            .show(ui, |ui| ui.label("Expanded contents"))
                            .openness;
                    });
                },
            );
            let mesh = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .expect("Header must paint the SVG chevron");
            let edge = mesh.vertices[1].pos - mesh.vertices[0].pos;
            assert!(
                (edge.angle() - openness * std::f32::consts::FRAC_PI_2).abs() < 0.001,
                "Icon rotation must follow body expansion: {openness}"
            );
            (openness, output.textures_delta.set.len())
        };
        assert_eq!(frame(false).0, 0.0);
        for expanded in [true, false] {
            let mut intermediate_frames = 0;
            let mut final_openness = 0.0;
            for _ in 0..30 {
                let (openness, texture_updates) = frame(expanded);
                if openness > 0.0 && openness < 1.0 {
                    intermediate_frames += 1;
                }
                assert_eq!(texture_updates, 0, "Rotation must reuse the SVG texture");
                final_openness = openness;
            }
            assert!(
                intermediate_frames > 1,
                "Rotation must not snap between endpoints"
            );
            assert_eq!(final_openness, if expanded { 1.0 } else { 0.0 });
        }
    }
}
