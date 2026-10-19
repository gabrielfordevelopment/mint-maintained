use std::{ffi::CStr, os::windows::ffi::OsStrExt, path::Path, process::Command};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HMODULE, WAIT_OBJECT_0},
        System::{
            LibraryLoader::{GetProcAddress, LoadLibraryW},
            SystemInformation::GetSystemDirectoryW,
            Threading::{
                CreateThread, GetExitCodeThread, STACK_SIZE_PARAM_IS_A_RESERVATION,
                WaitForSingleObject,
            },
        },
    },
    core::{PCSTR, PCWSTR},
};

fn run_child(filename: &str) {
    run_child_with_small_stack(filename, false);
}

fn run_child_with_small_stack(filename: &str, small_stack: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(filename);
    std::fs::copy(env!("CARGO_CDYLIB_FILE_HOOK_hook"), &path).unwrap();
    // The DLL queues an APC on its loading thread. Never unload that DLL while the
    // callback could still run; a disposable process owns the complete lifetime.
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native::native_proxy_child", "--nocapture"])
        .env("MINT_HOOK_PROXY_CHILD", &path)
        .env(
            "MINT_HOOK_PROXY_SMALL_STACK",
            if small_stack { "1" } else { "0" },
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{filename}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.stderr.is_empty() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn steam_proxy_allows_small_stack_thread_notifications() {
    run_child_with_small_stack("X3DAudio1_7.DLL", true);
}

#[test]
fn store_proxy_allows_small_stack_thread_notifications() {
    run_child_with_small_stack("d3d9.dll", true);
}

unsafe extern "system" fn small_stack_thread(_: *mut std::ffi::c_void) -> u32 {
    42
}

fn assert_small_stack_thread_runs() {
    // Windows invokes every loaded DLL's thread-attach and thread-detach callbacks
    // on this small stack, before and after our otherwise trivial entry point.
    unsafe {
        let thread = CreateThread(
            None,
            64 * 1024,
            Some(small_stack_thread),
            None,
            STACK_SIZE_PARAM_IS_A_RESERVATION,
            None,
        )
        .unwrap();
        assert_eq!(WaitForSingleObject(thread, 10_000), WAIT_OBJECT_0);
        let mut exit_code = 0;
        GetExitCodeThread(thread, &mut exit_code).unwrap();
        CloseHandle(thread).unwrap();
        assert_eq!(exit_code, 42);
    }
}

#[test]
fn steam_proxy_validates_native_audio_library_and_forwards() {
    run_child("X3DAudio1_7.DLL");
}

#[test]
fn store_proxy_validates_native_d3d9_library_and_forwards() {
    run_child("d3d9.dll");
}

#[test]
fn unknown_proxy_filename_rejects_native_attachment() {
    run_child("hook.dll");
}

fn load(path: &Path) -> windows::core::Result<HMODULE> {
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe { LoadLibraryW(PCWSTR(path.as_ptr())) }
}

unsafe fn address(module: HMODULE, name: &CStr) -> unsafe extern "system" fn() -> isize {
    unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }
        .unwrap_or_else(|| panic!("missing native export {name:?}"))
}

unsafe fn assert_forward(proxy: HMODULE, original: HMODULE, name: &CStr) {
    let stub = unsafe { address(proxy, name) } as *const u8;
    // The production x64 trampoline is FF 25 disp32: jmp [rip + disp32].
    // Validate every slot without calling graphics/audio functions with fabricated inputs.
    unsafe {
        assert_eq!(std::slice::from_raw_parts(stub, 2), [0xff, 0x25]);
        let displacement = std::ptr::read_unaligned(stub.add(2).cast::<i32>());
        let slot = stub
            .add(6)
            .wrapping_offset(displacement as isize)
            .cast::<usize>();
        assert_eq!(
            std::ptr::read_unaligned(slot),
            address(original, name) as *const () as usize,
            "wrong native forward for {name:?}"
        );
    }
}

#[test]
fn native_proxy_child() {
    let Some(path) = std::env::var_os("MINT_HOOK_PROXY_CHILD") else {
        return;
    };
    let path = Path::new(&path);
    if path.file_name().unwrap() == "hook.dll" {
        assert!(load(path).is_err());
        return;
    }

    let mut system_directory = [0u16; 32768];
    let length = unsafe { GetSystemDirectoryW(Some(&mut system_directory)) } as usize;
    assert!(length > 0 && length < system_directory.len());
    let system_path =
        std::path::PathBuf::from(String::from_utf16_lossy(&system_directory[..length]))
            .join(path.file_name().unwrap());
    let original = match load(&system_path) {
        Ok(module) => module,
        Err(error) => {
            // Legacy DirectX audio is not installed on every Windows test host.
            // Verify native attachment rejects that missing prerequisite cleanly.
            assert!(load(path).is_err());
            eprintln!(
                "System library unavailable ({error}); verified attachment rejection. \
                 Native forwarding requires {}.",
                system_path.display()
            );
            return;
        }
    };
    let proxy = load(path).unwrap();
    if std::env::var("MINT_HOOK_PROXY_SMALL_STACK").as_deref() == Ok("1") {
        assert_small_stack_thread_runs();
    }

    // No alertable wait: the real game-specific hook is deliberately not dispatched.
    // Only the production DLL's system-library forwards are exercised here.
    unsafe {
        if path.extension().unwrap() == "DLL" {
            for name in [c"X3DAudioCalculate", c"X3DAudioInitialize"] {
                assert_forward(proxy, original, name);
            }
            type Initialize = unsafe extern "system" fn(u32, f32, *mut u8);
            let original_initialize: Initialize =
                std::mem::transmute(address(original, c"X3DAudioInitialize"));
            let proxy_initialize: Initialize =
                std::mem::transmute(address(proxy, c"X3DAudioInitialize"));
            for (channels, speed) in [(3, 343.5), (0x3f, 250.0)] {
                let mut expected = [0u8; 20];
                let mut actual = [0u8; 20];
                original_initialize(channels, speed, expected.as_mut_ptr());
                proxy_initialize(channels, speed, actual.as_mut_ptr());
                assert_eq!(actual, expected, "integer, float and pointer arguments");
                assert!(actual.iter().any(|byte| *byte != 0));
            }
        } else {
            type GetStatus = unsafe extern "system" fn() -> u32;
            let original_status: GetStatus =
                std::mem::transmute(address(original, c"D3DPERF_GetStatus"));
            let proxy_status: GetStatus = std::mem::transmute(address(proxy, c"D3DPERF_GetStatus"));
            assert_eq!(proxy_status(), original_status());
            for name in [
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
                c"Direct3DCreate9",
                c"Direct3DCreate9Ex",
                c"Direct3D9EnableMaximizedWindowedModeShim",
            ] {
                assert_forward(proxy, original, name);
            }
        }
    }
}
