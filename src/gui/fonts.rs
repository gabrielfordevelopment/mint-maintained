pub(super) fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let name = "noto-sans-cjk";
    fonts.font_data.insert(
        name.into(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansCJKsc-Regular.otf"
        ))
        .into(),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(name.into());
    }
    ctx.set_fonts(fonts);
}
