//! Windows System Tray Icon dla OpenCode-RS.
//! Umożliwia działanie w tle (daemon), zarządzanie serwerem,
//! szybkie otwieranie Web UI w przeglądarce, terminala TUI oraz folderu danych.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

pub static TRAY_RUNNING: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub fn run_tray_icon(port: u16, _work_dir: PathBuf) -> anyhow::Result<()> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
        NIF_ICON, NIF_MESSAGE, NIF_TIP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos,
        GetMessageW, LoadIconW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
        TrackPopupMenu, TranslateMessage, CreateWindowExW, DefWindowProcW,
        AppendMenuW, HMENU, IDI_APPLICATION, MF_SEPARATOR, MF_STRING,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_COMMAND, WM_DESTROY, WM_LBUTTONDBLCLK,
        WM_RBUTTONUP, WM_USER, WNDCLASSW,
    };

    const WM_TRAYICON: u32 = WM_USER + 1;
    const ID_OPEN_WEB: usize = 1001;
    const ID_OPEN_TUI: usize = 1002;
    const ID_OPEN_DIR: usize = 1003;
    const ID_TOGGLE_AUTOSTART: usize = 1004;
    const ID_EXIT: usize = 1005;

    // Przekaż port i work_dir przez thread local lub static
    PORT_STORAGE.store(port, Ordering::SeqCst);
    TRAY_RUNNING.store(true, Ordering::SeqCst);

    unsafe {
        let class_name: Vec<u16> = "OpenCodeRSTrayClass\0".encode_utf16().collect();

        unsafe extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            match msg {
                WM_TRAYICON => {
                    let event = lparam as u32;
                    if event == WM_RBUTTONUP {
                        let mut pt = POINT { x: 0, y: 0 };
                        GetCursorPos(&mut pt);
                        SetForegroundWindow(hwnd);

                        let hmenu: HMENU = CreatePopupMenu();
                        let port = PORT_STORAGE.load(Ordering::SeqCst);

                        let web_title: Vec<u16> = format!("🌐 Otwórz Web UI (port {})\0", port).encode_utf16().collect();
                        let tui_title: Vec<u16> = "💻 Otwórz Terminal TUI (F3 IDE)\0".encode_utf16().collect();
                        let dir_title: Vec<u16> = "📁 Otwórz katalog ~/.opencode-rs\0".encode_utf16().collect();
                        let auto_title: Vec<u16> = "🔄 Autostart z Windows (Włącz/Wyłącz)\0".encode_utf16().collect();
                        let exit_title: Vec<u16> = "❌ Zakończ OpenCode-RS\0".encode_utf16().collect();

                        AppendMenuW(hmenu, MF_STRING, ID_OPEN_WEB, web_title.as_ptr());
                        AppendMenuW(hmenu, MF_STRING, ID_OPEN_TUI, tui_title.as_ptr());
                        AppendMenuW(hmenu, MF_STRING, ID_OPEN_DIR, dir_title.as_ptr());
                        AppendMenuW(hmenu, MF_SEPARATOR, 0, null_mut());
                        AppendMenuW(hmenu, MF_STRING, ID_TOGGLE_AUTOSTART, auto_title.as_ptr());
                        AppendMenuW(hmenu, MF_SEPARATOR, 0, null_mut());
                        AppendMenuW(hmenu, MF_STRING, ID_EXIT, exit_title.as_ptr());

                        TrackPopupMenu(
                            hmenu,
                            TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                            pt.x,
                            pt.y,
                            0,
                            hwnd,
                            null_mut(),
                        );
                        DestroyMenu(hmenu);
                    } else if event == WM_LBUTTONDBLCLK {
                        // Podwójne kliknięcie: natychmiast otwórz Web UI w przeglądarce
                        let port = PORT_STORAGE.load(Ordering::SeqCst);
                        open_browser(port);
                    }
                    0
                }
                WM_COMMAND => {
                    let cmd_id = (wparam & 0xFFFF) as usize;
                    let port = PORT_STORAGE.load(Ordering::SeqCst);
                    match cmd_id {
                        ID_OPEN_WEB => {
                            open_browser(port);
                        }
                        ID_OPEN_TUI => {
                            let _ = std::process::Command::new("cmd")
                                .args(["/c", "start", "opencode.exe"])
                                .spawn();
                        }
                        ID_OPEN_DIR => {
                            if let Some(user_dirs) = directories::UserDirs::new() {
                                let dir = user_dirs.home_dir().join(".opencode-rs");
                                let _ = std::process::Command::new("explorer")
                                    .arg(dir)
                                    .spawn();
                            }
                        }
                        ID_TOGGLE_AUTOSTART => {
                            toggle_autostart();
                        }
                        ID_EXIT => {
                            TRAY_RUNNING.store(false, Ordering::SeqCst);
                            PostQuitMessage(0);
                        }
                        _ => {}
                    }
                    0
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: null_mut(),
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        );

        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);

        let tip_str = format!("OpenCode-RS (Port: {})\0", port);
        let tip_utf16: Vec<u16> = tip_str.encode_utf16().collect();
        let len = tip_utf16.len().min(nid.szTip.len() - 1);
        nid.szTip[..len].copy_from_slice(&tip_utf16[..len]);

        Shell_NotifyIconW(NIM_ADD, &nid);

        eprintln!("🚀 [OpenCode-RS] Ikonka w zasobniku systemowym aktywna (port {port}).");
        eprintln!("💡 Kliknij dwukrotnie w ikonkę w trayu, aby otworzyć Web UI.");

        // Pętla komunikatów Win32
        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Czyszczenie przy wyjściu
        Shell_NotifyIconW(NIM_DELETE, &nid);
        DestroyWindow(hwnd);
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn run_tray_icon(port: u16, _work_dir: PathBuf) -> anyhow::Result<()> {
    eprintln!("System tray jest obsługiwany natywnie w systemie Windows.");
    eprintln!("Uruchomiono serwer Web Companion na porcie {port}.");
    Ok(())
}

use std::sync::atomic::AtomicU16;
static PORT_STORAGE: AtomicU16 = AtomicU16::new(8765);

/// Otwiera domyślną przeglądarkę pod adresem Web UI
pub fn open_browser(port: u16) {
    let url = format!("http://127.0.0.1:{}", port);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", &url])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn();
    }
}

/// Przełącza autostart z systemem Windows (wpis w rejestrze HKCU)
pub fn toggle_autostart() {
    #[cfg(windows)]
    {
        let check_cmd = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "OpenCodeRS"])
            .output();

        if let Ok(output) = check_cmd {
            if output.status.success() {
                // Usuń autostart
                let _ = std::process::Command::new("reg")
                    .args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "OpenCodeRS", "/f"])
                    .output();
                eprintln!("⏹️ [OpenCode-RS] Autostart z systemem Windows wyłączony.");
            } else if let Ok(exe_path) = std::env::current_exe() {
                // Dodaj autostart z flagą --tray
                let reg_val = format!("\"{}\" --tray", exe_path.display());
                let _ = std::process::Command::new("reg")
                    .args([
                        "add",
                        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                        "/v",
                        "OpenCodeRS",
                        "/t",
                        "REG_SZ",
                        "/d",
                        &reg_val,
                        "/f",
                    ])
                    .output();
                eprintln!("✅ [OpenCode-RS] Autostart z systemem Windows włączony (start w trayu).");
            }
        }
    }
}
