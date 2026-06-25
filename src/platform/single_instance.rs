//! Single-instance enforcement via named mutex + named pipe IPC.
//!
//! First instance: creates mutex, listens on pipe for file paths from secondary instances.
//! Second instance: fails to create mutex, sends its file path via pipe, exits.

#![allow(non_snake_case)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

type HANDLE = isize;
type BOOL = i32;
type DWORD = u32;

const ERROR_ALREADY_EXISTS: u32 = 183;
const PIPE_ACCESS_INBOUND: DWORD = 0x00000001;
const FILE_FLAG_OVERLAPPED: DWORD = 0x40000000;
const PIPE_TYPE_BYTE: DWORD = 0x00000000;
const PIPE_WAIT: DWORD = 0x00000000;
const NMPWAIT_USE_DEFAULT_WAIT: DWORD = 0x00000000;
const INFINITE: DWORD = 0xFFFFFFFF;
const WAIT_OBJECT_0: DWORD = 0;
const WAIT_TIMEOUT: DWORD = 0x00000102;

#[repr(C)]
struct OVERLAPPED {
    internal: usize,
    internal_high: usize,
    pointer: *mut u8,
    event: HANDLE,
}

extern "system" {
    fn CreateMutexW(lpMutexAttributes: *const u8, bInitialOwner: BOOL, lpName: *const u16) -> HANDLE;
    fn GetLastError() -> DWORD;
    fn CloseHandle(hObject: HANDLE) -> BOOL;
    fn CreateNamedPipeW(
        lpName: *const u16,
        dwOpenMode: DWORD,
        dwPipeMode: DWORD,
        nMaxInstances: DWORD,
        nOutBufferSize: DWORD,
        nInBufferSize: DWORD,
        nDefaultTimeOut: DWORD,
        lpSecurityAttributes: *const u8,
    ) -> HANDLE;
    fn ConnectNamedPipe(hNamedPipe: HANDLE, lpOverlapped: *mut OVERLAPPED) -> BOOL;
    fn ReadFile(
        hFile: HANDLE,
        lpBuffer: *mut u8,
        nNumberOfBytesToRead: DWORD,
        lpNumberOfBytesRead: *mut DWORD,
        lpOverlapped: *mut OVERLAPPED,
    ) -> BOOL;
    fn DisconnectNamedPipe(hNamedPipe: HANDLE) -> BOOL;
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: DWORD,
        dwShareMode: DWORD,
        lpSecurityAttributes: *const u8,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes: DWORD,
        hTemplateFile: HANDLE,
    ) -> HANDLE;
    fn WriteFile(
        hFile: HANDLE,
        lpBuffer: *const u8,
        nNumberOfBytesToWrite: DWORD,
        lpNumberOfBytesWritten: *mut DWORD,
        lpOverlapped: *mut OVERLAPPED,
    ) -> BOOL;
    fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
    fn CreateEventW(lpEventAttributes: *const u8, bManualReset: BOOL, bInitialState: BOOL, lpName: *const u16) -> HANDLE;
    fn SetEvent(hEvent: HANDLE) -> BOOL;
    fn SetForegroundWindow(hWnd: HANDLE) -> BOOL;
    fn ShowWindow(hWnd: HANDLE, nCmdShow: i32) -> BOOL;
    fn IsIconic(hWnd: HANDLE) -> BOOL;
}

const GENERIC_WRITE: DWORD = 0x40000000;
const OPEN_EXISTING: DWORD = 3;
const SW_RESTORE: i32 = 9;
const SW_SHOW: i32 = 5;

const MUTEX_NAME: &str = "DIAVLO_PLAYER_SingleInstance_Mutex";
const PIPE_NAME: &str = r"\\.\pipe\DIAVLO_PLAYER_IPC";

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Try to create the single-instance mutex.
/// Returns `Some(handle)` if this is the first instance, or `None` if another instance exists.
pub fn try_lock_single_instance() -> Option<HANDLE> {
    let name = to_wide(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle == 0 {
        return None;
    }
    let err = unsafe { GetLastError() };
    if err == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle); }
        None
    } else {
        Some(handle)
    }
}

/// Send a file path to the existing instance via named pipe.
/// Returns true on success.
pub fn send_path_to_primary(path: &str) -> bool {
    let pipe_name = to_wide(PIPE_NAME);
    let handle = unsafe {
        CreateFileW(
            pipe_name.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if handle == -1 {
        return false;
    }

    // Write: 4 bytes length (little-endian) + UTF-8 path
    let path_bytes = path.as_bytes();
    let len = path_bytes.len() as u32;
    let len_bytes = len.to_le_bytes();
    let mut written: DWORD = 0;
    let ok1 = unsafe {
        WriteFile(handle, len_bytes.as_ptr(), 4, &mut written, std::ptr::null_mut())
    };
    let ok2 = unsafe {
        WriteFile(handle, path_bytes.as_ptr(), len, &mut written, std::ptr::null_mut())
    };
    unsafe { CloseHandle(handle); }
    ok1 != 0 && ok2 != 0
}

/// Start listening for file paths from secondary instances.
/// Returns a handle to the pipe. Call `poll_pipe` to check for messages.
pub fn start_pipe_server() -> Option<HANDLE> {
    let pipe_name = to_wide(PIPE_NAME);
    let handle = unsafe {
        CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_WAIT,
            1, // max instances
            0,
            4096,
            0,
            std::ptr::null(),
        )
    };
    if handle == -1 {
        return None;
    }
    // Start an overlapped connection wait
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event == 0 {
        unsafe { CloseHandle(handle); }
        return None;
    }
    let mut overlapped = OVERLAPPED {
        internal: 0,
        internal_high: 0,
        pointer: std::ptr::null_mut(),
        event,
    };
    let connected = unsafe { ConnectNamedPipe(handle, &mut overlapped) };
    if connected == 0 {
        let err = unsafe { GetLastError() };
        // ERROR_PIPE_CONNECTED (535) means a client already connected
        if err != 535 {
            unsafe { CloseHandle(handle); }
            return None;
        }
    }
    Some(handle)
}

/// Poll the pipe for incoming file paths (non-blocking).
/// Returns `Some(path)` if a message was received, `None` if no message yet.
pub fn poll_pipe(pipe: HANDLE) -> Option<String> {
    // Check if the overlapped connect has completed
    let result = unsafe { WaitForSingleObject(pipe, 0) };
    if result != WAIT_OBJECT_0 {
        return None;
    }

    // Data available — read length prefix
    let mut len_buf = [0u8; 4];
    let mut bytes_read: DWORD = 0;
    let ok = unsafe {
        ReadFile(pipe, len_buf.as_mut_ptr(), 4, &mut bytes_read, std::ptr::null_mut())
    };
    if ok == 0 || bytes_read != 4 {
        // Reset for next connection
        unsafe { DisconnectNamedPipe(pipe); }
        // Re-arm
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        let mut overlapped = OVERLAPPED {
            internal: 0,
            internal_high: 0,
            pointer: std::ptr::null_mut(),
            event,
        };
        unsafe { ConnectNamedPipe(pipe, &mut overlapped); }
        return None;
    }

    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 32000 {
        unsafe { DisconnectNamedPipe(pipe); }
        return None;
    }

    let mut buf = vec![0u8; len];
    let mut bytes_read2: DWORD = 0;
    let ok2 = unsafe {
        ReadFile(pipe, buf.as_mut_ptr(), len as DWORD, &mut bytes_read2, std::ptr::null_mut())
    };
    if ok2 == 0 || bytes_read2 as usize != len {
        unsafe { DisconnectNamedPipe(pipe); }
        return None;
    }

    // Reset for next connection
    unsafe { DisconnectNamedPipe(pipe); }
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    let mut overlapped = OVERLAPPED {
        internal: 0,
        internal_high: 0,
        pointer: std::ptr::null_mut(),
        event,
    };
    unsafe { ConnectNamedPipe(pipe, &mut overlapped); }

    String::from_utf8(buf).ok()
}

/// Bring window to front safely (restore if minimized, don't steal focus aggressively).
pub fn bring_to_front(hwnd: isize) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        } else {
            ShowWindow(hwnd, SW_SHOW);
        }
        SetForegroundWindow(hwnd);
    }
}
