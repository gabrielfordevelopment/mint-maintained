use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;

type Window = *mut c_void;
type Procedure = unsafe extern "system" fn(Window, u32, usize, isize, usize, usize) -> isize;

#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(window: Window, procedure: Procedure, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(window: Window, procedure: Procedure, id: usize) -> i32;
    fn DefSubclassProc(window: Window, message: u32, wparam: usize, lparam: isize) -> isize;
}

pub(super) fn install(cc: &eframe::CreationContext<'_>) {
    let Ok(handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let context = Box::into_raw(Box::new(cc.egui_ctx.clone()));
    // The callback owns the context until this window receives WM_NCDESTROY.
    unsafe {
        if SetWindowSubclass(
            handle.hwnd.get() as Window,
            window_proc,
            1,
            context as usize,
        ) == 0
        {
            drop(Box::from_raw(context));
            tracing::warn!("Could not install native popup dismissal");
        }
    }
}

unsafe extern "system" fn window_proc(
    window: Window,
    message: u32,
    wparam: usize,
    lparam: isize,
    id: usize,
    data: usize,
) -> isize {
    const WM_NCDESTROY: u32 = 0x0082;
    const WM_NCLBUTTONDOWN: u32 = 0x00A1;
    const WM_NCRBUTTONDOWN: u32 = 0x00A4;
    const WM_NCMBUTTONDOWN: u32 = 0x00A7;
    const WM_KILLFOCUS: u32 = 0x0008;
    // Windows calls this on the GUI thread with the context registered above.
    unsafe {
        if message == WM_NCDESTROY {
            RemoveWindowSubclass(window, window_proc, id);
            drop(Box::from_raw(data as *mut egui::Context));
        } else if matches!(
            message,
            WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN | WM_NCMBUTTONDOWN | WM_KILLFOCUS
        ) {
            let ctx = &*(data as *const egui::Context);
            super::dismiss_popups(ctx);
            ctx.request_repaint();
        }
        DefSubclassProc(window, message, wparam, lparam)
    }
}
