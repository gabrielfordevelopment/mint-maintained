use std::{
    ffi::c_void,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
};

use windows::{
    Win32::{
        Foundation::HMODULE,
        System::{
            Diagnostics::Debug::OutputDebugStringW,
            LibraryLoader::{GetModuleFileNameW, GetProcAddress, LoadLibraryW},
            SystemInformation::GetSystemDirectoryW,
            SystemServices::DLL_PROCESS_ATTACH,
            Threading::{GetCurrentThread, QueueUserAPC},
        },
    },
    core::{BOOL, PCSTR, PCWSTR},
};

use super::{AUDIO_EXPORTS, D3D9_EXPORTS, Loader, Target, prepare};

// Written once under the loader lock, before Windows exposes this module's exports.
// Unselected exports fail explicitly if called instead of jumping through a null address.
static mut FORWARDS: [usize; AUDIO_EXPORTS.len() + D3D9_EXPORTS.len()] =
    [0; AUDIO_EXPORTS.len() + D3D9_EXPORTS.len()];

unsafe extern "system" fn unsupported_export() -> ! {
    std::process::abort()
}

macro_rules! forward {
    ($name:ident, $index:expr) => {
        // A tail jump preserves all argument registers, stack arguments and return values.
        // These exports have different native signatures and must never call a zero-arg wrapper.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $name() {
            core::arch::naked_asm!(
                "jmp qword ptr [rip + {table} + {offset}]",
                table = sym FORWARDS,
                offset = const $index * size_of::<usize>(),
            );
        }
    };
}

forward!(X3DAudioCalculate, 0);
forward!(X3DAudioInitialize, 1);
forward!(Direct3DShaderValidatorCreate9, 2);
forward!(PSGPError, 3);
forward!(PSGPSampleTexture, 4);
forward!(D3DPERF_BeginEvent, 5);
forward!(D3DPERF_EndEvent, 6);
forward!(D3DPERF_GetStatus, 7);
forward!(D3DPERF_QueryRepeatFrame, 8);
forward!(D3DPERF_SetMarker, 9);
forward!(D3DPERF_SetOptions, 10);
forward!(D3DPERF_SetRegion, 11);
forward!(DebugSetLevel, 12);
forward!(DebugSetMute, 13);
forward!(Direct3D9EnableMaximizedWindowedModeShim, 14);
forward!(Direct3DCreate9, 15);
forward!(Direct3DCreate9Ex, 16);

struct Module(HMODULE);

impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::FreeLibrary(self.0);
        }
    }
}

struct SystemLoader;

impl Loader for SystemLoader {
    type Module = Module;

    fn load(&mut self, target: Target) -> Result<Module, String> {
        // Always use the system directory; loading by basename would find this proxy again.
        let mut buffer = [0u16; 32768];
        let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
        if length == 0 || length >= buffer.len() {
            return Err("could not determine the system DLL directory".into());
        }
        let path =
            PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length])).join(target.filename());
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe { LoadLibraryW(PCWSTR(path.as_ptr())) }
            .map(Module)
            .map_err(|error| format!("could not load {}: {error}", target.filename()))
    }

    fn resolve(&mut self, module: &Module, name: &std::ffi::CStr) -> Option<usize> {
        unsafe { GetProcAddress(module.0, PCSTR(name.as_ptr().cast())) }
            .map(|address| address as usize)
    }
}

unsafe extern "system" fn initialize_hook(_: usize) {
    super::super::init();
}

fn attach(instance: HMODULE) -> Result<(), String> {
    let mut buffer = [0u16; 32768];
    let length = unsafe { GetModuleFileNameW(Some(instance), &mut buffer) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err("could not determine the hook DLL filename".into());
    }
    let filename = String::from_utf16_lossy(&buffer[..length]);
    let filename = filename.rsplit(['\\', '/']).next().unwrap_or_default();
    let prepared = prepare(filename, &mut SystemLoader)?;
    unsafe {
        let table = core::ptr::addr_of_mut!(FORWARDS).cast::<usize>();
        for index in 0..AUDIO_EXPORTS.len() + D3D9_EXPORTS.len() {
            table
                .add(index)
                .write(unsupported_export as *const () as usize);
        }
        for (index, address) in prepared.addresses.iter().enumerate() {
            table.add(prepared.target.offset() + index).write(*address);
        }
        if QueueUserAPC(Some(initialize_hook), GetCurrentThread(), 0) == 0 {
            return Err("could not queue MINT hook initialization".into());
        }
    }
    // Keep the selected system DLL loaded until process exit. Freeing it from DllMain
    // during teardown is unnecessary and can invalidate code still used by other DLLs.
    std::mem::forget(prepared.module);
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(instance: HMODULE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH
        && let Err(error) = attach(instance)
    {
        let message: Vec<u16> = format!("MINT hook proxy: {error}\n\0")
            .encode_utf16()
            .collect();
        unsafe { OutputDebugStringW(PCWSTR(message.as_ptr())) };
        return false.into();
    }
    true.into()
}
