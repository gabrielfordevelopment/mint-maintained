//! Select and validate only the system DLL corresponding to this proxy's filename.

use std::ffi::CStr;

#[cfg(not(test))]
#[path = "proxy/windows.rs"]
mod windows;

const AUDIO_EXPORTS: &[&CStr] = &[c"X3DAudioCalculate", c"X3DAudioInitialize"];
const D3D9_EXPORTS: &[&CStr] = &[
    c"Direct3DShaderValidatorCreate9",
    c"PSGPError",
    c"PSGPSampleTexture",
    c"D3DPERF_BeginEvent",
    c"D3DPERF_EndEvent",
    c"D3DPERF_GetStatus",
    c"D3DPERF_QueryRepeatFrame",
    c"D3DPERF_SetMarker",
    c"D3DPERF_SetOptions",
    c"D3DPERF_SetRegion",
    c"DebugSetLevel",
    c"DebugSetMute",
    c"Direct3D9EnableMaximizedWindowedModeShim",
    c"Direct3DCreate9",
    c"Direct3DCreate9Ex",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Audio,
    D3D9,
}

impl Target {
    fn from_filename(filename: &str) -> Option<Self> {
        if filename.eq_ignore_ascii_case("x3daudio1_7.dll") {
            Some(Self::Audio)
        } else if filename.eq_ignore_ascii_case("d3d9.dll") {
            Some(Self::D3D9)
        } else {
            None
        }
    }

    fn filename(self) -> &'static str {
        match self {
            Self::Audio => "x3daudio1_7.dll",
            Self::D3D9 => "d3d9.dll",
        }
    }

    fn exports(self) -> &'static [&'static CStr] {
        match self {
            Self::Audio => AUDIO_EXPORTS,
            Self::D3D9 => D3D9_EXPORTS,
        }
    }

    fn offset(self) -> usize {
        match self {
            Self::Audio => 0,
            Self::D3D9 => AUDIO_EXPORTS.len(),
        }
    }
}

trait Loader {
    type Module;

    fn load(&mut self, target: Target) -> Result<Self::Module, String>;
    fn resolve(&mut self, module: &Self::Module, name: &CStr) -> Option<usize>;
}

struct Prepared<M> {
    // Own the library until all forwards have been published, or drop it on failure.
    module: M,
    target: Target,
    addresses: Vec<usize>,
}

fn prepare<L: Loader>(filename: &str, loader: &mut L) -> Result<Prepared<L::Module>, String> {
    let target = Target::from_filename(filename)
        .ok_or_else(|| format!("unsupported hook DLL filename: {filename}"))?;
    let module = loader.load(target)?;
    let addresses = target
        .exports()
        .iter()
        .map(|name| {
            loader
                .resolve(&module, name)
                .filter(|p| *p != 0)
                .ok_or_else(|| {
                    format!(
                        "{} is missing {}",
                        target.filename(),
                        name.to_string_lossy()
                    )
                })
        })
        .collect::<Result<_, _>>()?;
    Ok(Prepared {
        module,
        target,
        addresses,
    })
}

#[cfg(test)]
#[path = "proxy/tests.rs"]
mod tests;
