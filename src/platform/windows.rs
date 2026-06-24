#![allow(non_snake_case)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

type HWND = isize;
type BOOL = i32;
type HRESULT = i32;
type DWORD = u32;

const WS_CAPTION: isize     = 0x00C00000;
const WS_THICKFRAME: isize  = 0x00040000;
const WS_MINIMIZEBOX: isize = 0x00020000;
const WS_MAXIMIZEBOX: isize = 0x00010000;
const WS_SYSMENU: isize     = 0x00080000;
const WS_POPUP: isize       = 0x80000000;
const WS_VISIBLE: isize     = 0x10000000;
const WS_EX_APPWINDOW: isize = 0x00040000;

const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOZORDER: u32 = 0x0004;
const GWL_STYLE: i32 = -16;
const GWL_EXSTYLE: i32 = -20;
const DWM_BB_ENABLE: DWORD = 0x00000001;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

#[repr(C)] struct DWM_BLURBEHIND { dwFlags: DWORD, fEnable: BOOL, hRgnBlur: isize, fTransitionOnMaximized: BOOL }
#[repr(C)] struct MARGINS { cxLeftWidth: i32, cxRightWidth: i32, cyTopHeight: i32, cyBottomHeight: i32 }

extern "system" {
    fn SetWindowLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
    fn GetWindowLongPtrW(hWnd: HWND, nIndex: i32) -> isize;
    fn SetWindowPos(hWnd: HWND, hWndInsertAfter: isize, X: i32, Y: i32, cx: i32, cy: i32, uFlags: u32) -> BOOL;
    fn DwmEnableBlurBehindWindow(hWnd: HWND, pBlurBehind: *const DWM_BLURBEHIND) -> HRESULT;
    fn DwmExtendFrameIntoClientArea(hWnd: HWND, pMarInset: *const MARGINS) -> HRESULT;
    fn SetProcessDpiAwarenessContext(value: isize) -> BOOL;
}

pub unsafe fn hwnd_from_slint(window: &slint::Window) -> Option<HWND> {
    // slint::WindowHandle implements HasWindowHandle trait
    let wh = window.window_handle();
    let raw = HasWindowHandle::window_handle(&wh).ok()?;
    match raw.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

pub unsafe fn apply_frameless_glass(hwnd: HWND) {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let old = GetWindowLongPtrW(hwnd, GWL_STYLE);
    let new = (old & !(WS_CAPTION | WS_THICKFRAME)) | WS_POPUP | WS_VISIBLE | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU;
    SetWindowLongPtrW(hwnd, GWL_STYLE, new);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, WS_EX_APPWINDOW);
    let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
    let b = DWM_BLURBEHIND { dwFlags: DWM_BB_ENABLE, fEnable: 1, hRgnBlur: 0, fTransitionOnMaximized: 0 };
    let _ = DwmEnableBlurBehindWindow(hwnd, &b);
    SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);
}

pub unsafe fn minimize_window(hwnd: HWND) {
    extern "system" { fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL; }
    ShowWindow(hwnd, 6);
}

pub unsafe fn maximize_restore_window(hwnd: HWND) {
    extern "system" { fn IsZoomed(hWnd: HWND) -> BOOL; fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL; }
    if IsZoomed(hwnd) != 0 { ShowWindow(hwnd, 1); } else { ShowWindow(hwnd, 3); }
}

pub unsafe fn close_window(hwnd: HWND) {
    extern "system" { fn PostMessageW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> BOOL; }
    PostMessageW(hwnd, 0x0010, 0, 0);
}