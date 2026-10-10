use std::{env, fs, path::PathBuf};

use image::codecs::ico::{IcoEncoder, IcoFrame};

fn main() {
    println!("cargo:rerun-if-changed=assets/brand/mint.svg");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    let tree = resvg::usvg::Tree::from_data(
        include_bytes!("assets/brand/mint.svg"),
        &resvg::usvg::Options::default(),
    )
    .expect("Application icon must be valid SVG");
    let mut frames = Vec::new();
    for size in [16, 20, 24, 32, 40, 48, 64, 128, 256] {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
        let transform = resvg::tiny_skia::Transform::from_scale(
            size as f32 / tree.size().width(),
            size as f32 / tree.size().height(),
        );
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let png = pixmap.encode_png().expect("Application icon must encode");
        if size == 256 {
            fs::write(output.join("mint.png"), &png).expect("Cannot write window icon");
        }
        frames.push(
            IcoFrame::with_encoded(png, size, size, image::ExtendedColorType::Rgba8).unwrap(),
        );
    }
    let ico = output.join("mint.ico");
    IcoEncoder::new(fs::File::create(&ico).expect("Cannot create Windows icon"))
        .encode_images(&frames)
        .expect("Cannot encode Windows icon");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set("ProductName", "MINT Maintained")
            .set("FileDescription", "MINT Maintained")
            .set_icon(ico.to_str().expect("Icon path must be valid UTF-8"))
            .compile()
            .expect("Cannot embed Windows application icon");
    }
}
