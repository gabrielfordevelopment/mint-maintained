use std::{cell::Cell, rc::Rc};

use super::*;

struct FakeModule(Rc<Cell<usize>>);

impl Drop for FakeModule {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[derive(Default)]
struct FakeLoader {
    unavailable: Option<Target>,
    missing: Option<&'static CStr>,
    zero_address: bool,
    loaded: Vec<Target>,
    resolved: Vec<String>,
    released: Rc<Cell<usize>>,
}

impl Loader for FakeLoader {
    type Module = FakeModule;

    fn load(&mut self, target: Target) -> Result<FakeModule, String> {
        self.loaded.push(target);
        if self.unavailable == Some(target) {
            Err(format!("could not load {}", target.filename()))
        } else {
            Ok(FakeModule(self.released.clone()))
        }
    }

    fn resolve(&mut self, _: &FakeModule, name: &CStr) -> Option<usize> {
        self.resolved.push(name.to_string_lossy().into_owned());
        if self.missing == Some(name) {
            None
        } else if self.zero_address {
            Some(0)
        } else {
            Some(self.resolved.len())
        }
    }
}

#[test]
fn steam_proxy_loads_without_d3d9() {
    let mut loader = FakeLoader {
        unavailable: Some(Target::D3D9),
        ..Default::default()
    };
    let prepared = prepare("x3daudio1_7.dll", &mut loader).unwrap();
    assert_eq!(loader.loaded, [Target::Audio]);
    assert_eq!(loader.resolved, ["X3DAudioCalculate", "X3DAudioInitialize"]);
    assert_eq!(prepared.target, Target::Audio);
    assert_eq!(prepared.target.offset(), 0);
    assert_eq!(prepared.addresses, [1, 2]);
    assert_eq!(loader.released.get(), 0);
    drop(prepared.module);
    assert_eq!(loader.released.get(), 1);
}

#[test]
fn steam_proxy_ignores_missing_wine_d3d9_export() {
    let mut loader = FakeLoader {
        missing: Some(c"Direct3D9EnableMaximizedWindowedModeShim"),
        ..Default::default()
    };
    assert!(prepare("X3DAudio1_7.DLL", &mut loader).is_ok());
    assert_eq!(loader.loaded, [Target::Audio]);
    assert!(
        !loader
            .resolved
            .iter()
            .any(|name| name.starts_with("Direct3D"))
    );
}

#[test]
fn microsoft_store_proxy_loads_without_audio() {
    let mut loader = FakeLoader {
        unavailable: Some(Target::Audio),
        ..Default::default()
    };
    let prepared = prepare("D3D9.dll", &mut loader).unwrap();
    assert_eq!(loader.loaded, [Target::D3D9]);
    assert_eq!(prepared.target, Target::D3D9);
    assert_eq!(prepared.target.offset(), AUDIO_EXPORTS.len());
    assert_eq!(prepared.addresses.len(), D3D9_EXPORTS.len());
    assert!(loader.resolved.contains(&"Direct3DCreate9Ex".into()));
    assert!(
        !loader
            .resolved
            .iter()
            .any(|name| name.starts_with("X3DAudio"))
    );
}

#[test]
fn missing_selected_library_returns_an_error_without_resolving_exports() {
    for target in [Target::Audio, Target::D3D9] {
        let mut loader = FakeLoader {
            unavailable: Some(target),
            ..Default::default()
        };
        let error = prepare(target.filename(), &mut loader).err().unwrap();
        assert!(error.contains(target.filename()));
        assert!(loader.resolved.is_empty());
        assert_eq!(loader.released.get(), 0);
    }
}

#[test]
fn every_missing_selected_export_returns_an_error_and_releases_the_library() {
    for target in [Target::Audio, Target::D3D9] {
        for &name in target.exports() {
            let mut loader = FakeLoader {
                missing: Some(name),
                ..Default::default()
            };
            let error = prepare(target.filename(), &mut loader).err().unwrap();
            assert!(error.contains(target.filename()));
            assert!(error.contains(name.to_str().unwrap()));
            assert_eq!(loader.released.get(), 1);
        }
    }
}

#[test]
fn null_export_address_is_rejected() {
    let mut loader = FakeLoader {
        zero_address: true,
        ..Default::default()
    };
    assert!(prepare("x3daudio1_7.dll", &mut loader).is_err());
    assert_eq!(loader.released.get(), 1);
}

#[test]
fn unknown_filename_does_not_load_any_library() {
    for filename in ["hook.dll", "", "d3d9.dll.bak", "FSD-Win64-Shipping.exe"] {
        let mut loader = FakeLoader::default();
        assert!(prepare(filename, &mut loader).is_err());
        assert!(loader.loaded.is_empty());
        assert!(loader.resolved.is_empty());
    }
}
