# UI asset and renderer notices

## Application icon

The MINT Maintained M monogram in `assets/brand/mint.svg` is original,
code-authored vector artwork distributed under the repository's MIT OR Apache-2.0
license. Build-generated PNG and ICO images are mechanical renders of that source.
It is separate from the Google Material Symbols used for UI controls below.

Windows resource embedding uses the build-only `winresource` crate under the
MIT license: https://github.com/BenjaminRi/winresource.
It is not linked into the application at runtime.

## Unicode font fallback

The desktop UI embeds the unmodified Noto Sans CJK SC Regular 2.004 font as a
fallback for characters absent from egui's default fonts, including Chinese,
Japanese and Korean. Copyright 2014-2021 Adobe (http://www.adobe.com/).
It is distributed under the SIL Open Font License 1.1;
the complete license is in `assets/fonts/OFL.txt` and embedded in Settings.

Source: https://github.com/notofonts/noto-cjk
Revision: `523d033d6cb47f4a80c58a35753646f5c3608a78` (`Sans2.004`)
Upstream path: `Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf`
SHA-256: `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`

The font is compiled into the application; rendering does not require network
access, system font installation, or an external file beside the executable.

## Desktop log and window support

The live log uses Chrono 0.4.31 (MIT OR Apache-2.0) for readable UTC timestamps.
Windows title-bar popup dismissal uses raw-window-handle 0.6.2
(MIT OR Apache-2.0 OR Zlib) to access the existing native window.
Both packages were already present in the dependency graph; their versions
are unchanged. Chrono disables default features and enables its std feature;
raw-window-handle has no runtime dependencies.

Sources: https://github.com/chronotope/chrono and
https://github.com/rust-windowing/raw-window-handle.
Their MIT license texts are included in `assets/icons/DEPENDENCY_LICENSES.txt`.

## Material Symbols

The icons in `assets/icons/*.svg` are Google Material Symbols Rounded,
outlined, weight 400, optical size 20, licensed under Apache-2.0.

Source: https://github.com/google/material-design-icons

Revision: `737e3324305806514d7909874fa1818ae1808232`

Upstream path: `symbols/web/<name>/materialsymbolsrounded/<name>_20px.svg`

Included symbols: add, content_copy, dark_mode, delete, desktop_windows,
drag_handle, error, file_copy, folder, keyboard_arrow_right, language, light_mode,
settings, warning.

Modification: an explicit white fill was added for runtime theme tinting.
The original vector geometry is unchanged. Each file identifies the change.
The full Apache-2.0 license is in `assets/icons/LICENSE`.

## SVG renderer

The icons are embedded in the executable and rendered locally with resvg.
Rasterized textures are cached at the current display scale; SVG files remain
the editable sources. No network request is needed to display icons.

resvg and usvg are used under their MIT license option. Text rendering,
system font discovery, and raster image decoding features are disabled.
License and copyright texts for packages introduced or updated by this
renderer dependency are in `assets/icons/DEPENDENCY_LICENSES.txt`.
These are supplementary notices for the UI changes, not an inventory of all
existing application dependencies.

These notices and license texts are also embedded in the application under
Settings > Third-party notices.
