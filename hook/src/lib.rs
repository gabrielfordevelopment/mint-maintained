mod hooks;
mod proxy;
mod ue;

use std::{io::BufReader, path::Path};

use anyhow::{Context, Result};
use fs_err as fs;
use hooks::{FnLoadGameFromMemory, FnSaveGameToMemory};
use mint_lib::mod_info::Meta;
use tracing::{error, info};

fn init() {
    let exe_path = std::env::current_exe().ok();
    if let Err(error) = setup_logging(exe_path.as_deref()) {
        // stderr still reaches a terminal or the Proton log when the log file is unavailable.
        let error = mint_lib::redaction::redact(&format!("{error:?}"));
        eprintln!("MINT hook: failed to set up logging: {error}");
    }

    // This runs from an APC, where an escaping panic would abort the game. A failed or
    // panicking initialization leaves the game running without (some) MINT hooks instead;
    // the panic itself is recorded by the logging panic hook.
    match std::panic::catch_unwind(|| unsafe { patch(exe_path.as_deref()) }) {
        Ok(Ok(())) => info!("hook initialized"),
        Ok(Err(error)) => error!("hook initialization failed: {error:?}"),
        Err(_) => error!("hook initialization panicked"),
    }
}

fn setup_logging(exe_path: Option<&Path>) -> Result<()> {
    let bin_dir = exe_path
        .and_then(Path::parent)
        .context("could not determine the game binary directory")?;
    mint_lib::setup_logging(bin_dir.join("mint_hook.log"), "hook")
}

static mut GLOBALS: Option<Globals> = None;

pub struct Globals {
    resolution: hook_resolvers::HookResolution,
    meta: Meta,
}

impl Globals {
    pub fn gmalloc(&self) -> &ue::FMalloc {
        unsafe {
            &**(self.resolution.core.as_ref().unwrap().gmalloc.0 as *const *const ue::FMalloc)
        }
    }
    pub fn fframe_step(&self) -> ue::FnFFrameStep {
        unsafe { std::mem::transmute(self.resolution.core.as_ref().unwrap().fframe_step.0) }
    }
    pub fn fframe_step_explicit_property(&self) -> ue::FnFFrameStepExplicitProperty {
        unsafe {
            std::mem::transmute(
                self.resolution
                    .core
                    .as_ref()
                    .unwrap()
                    .fframe_step_explicit_property
                    .0,
            )
        }
    }
    pub fn fname_to_string(&self) -> ue::FnFNameToString {
        unsafe { std::mem::transmute(self.resolution.core.as_ref().unwrap().fnametostring.0) }
    }
    pub fn fname_ctor_wchar(&self) -> ue::FnFNameCtorWchar {
        unsafe { std::mem::transmute(self.resolution.core.as_ref().unwrap().fname_ctor_wchar.0) }
    }
    pub fn uobject_base_utility_get_path_name(&self) -> ue::FnUObjectBaseUtilityGetPathName {
        unsafe {
            std::mem::transmute(
                self.resolution
                    .core
                    .as_ref()
                    .unwrap()
                    .uobject_base_utility_get_path_name
                    .0,
            )
        }
    }
    pub fn save_game_to_memory(&self) -> FnSaveGameToMemory {
        unsafe {
            std::mem::transmute(
                self.resolution
                    .save_game
                    .as_ref()
                    .unwrap()
                    .save_game_to_memory
                    .0,
            )
        }
    }
    pub fn load_game_from_memory(&self) -> FnLoadGameFromMemory {
        unsafe {
            std::mem::transmute(
                self.resolution
                    .save_game
                    .as_ref()
                    .unwrap()
                    .load_game_from_memory
                    .0,
            )
        }
    }
}

pub fn globals() -> &'static Globals {
    #[allow(static_mut_refs)]
    unsafe {
        GLOBALS.as_ref().unwrap()
    }
}

unsafe fn patch(exe_path: Option<&Path>) -> Result<()> {
    let exe_path = exe_path.context("could not determine the game executable")?;
    info!("game executable: {}", exe_path.display());

    let pak_path = exe_path
        .ancestors()
        .nth(3)
        .map(|p| p.join("Content/Paks/mods_P.pak"))
        .context("could not determine pak path")?;
    info!("reading integration metadata from {}", pak_path.display());
    let meta = read_meta(&pak_path)?;
    info!(
        "integration metadata: MINT {}, {} mods",
        meta.version,
        meta.mods.len()
    );

    info!("resolving hook targets in the game executable");
    let image = patternsleuth::process::internal::read_image()
        .context("could not read the game executable image")?;
    let resolution = image
        .resolve(hook_resolvers::HookResolution::resolver())
        .context("could not resolve hook targets")?;
    info!("PS scan: {:#x?}", resolution);

    unsafe {
        GLOBALS = Some(Globals { resolution, meta });

        hooks::initialize()
    }
}

fn read_meta(pak_path: &Path) -> Result<Meta> {
    let mut pak_reader = BufReader::new(fs::File::open(pak_path)?);
    let pak = repak::PakBuilder::new()
        .reader(&mut pak_reader)
        .context("could not read the mods pak index")?;
    let meta_buf = pak
        .get("meta", &mut pak_reader)
        .context("could not read integration metadata from the mods pak")?;
    postcard::from_bytes(&meta_buf)
        .context("could not decode integration metadata; reinstall mods with this MINT version")
}
