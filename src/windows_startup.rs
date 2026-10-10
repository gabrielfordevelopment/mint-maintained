use std::ffi::c_void;

type Handle = *mut c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn GetStdHandle(kind: u32) -> Handle;
    fn SetStdHandle(kind: u32, handle: Handle) -> i32;
    fn GetFileType(handle: Handle) -> u32;
}

pub fn attach_parent_console() {
    // Attach only to an existing caller's console; preserve inherited pipes/files.
    // These Win32 calls accept the documented standard-handle IDs, without dereferencing handles.
    unsafe {
        let redirected = [(-10_i32) as u32, (-11_i32) as u32, (-12_i32) as u32].map(|kind| {
            let handle = GetStdHandle(kind);
            (kind, handle, matches!(GetFileType(handle), 1 | 3))
        });
        if AttachConsole(u32::MAX) != 0 {
            for (kind, handle, preserve) in redirected {
                if preserve {
                    SetStdHandle(kind, handle);
                }
            }
        }
    }
}
