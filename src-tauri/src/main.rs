#![cfg_attr(all(target_os = "windows"), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

        unsafe {
            let console_window: HWND =
                windows_sys::Win32::UI::WindowsAndMessaging::GetConsoleWindow();
            if console_window != 0 {
                ShowWindow(console_window, SW_HIDE);
            }
        }
    }

    onvif_viewer_lib::run();
}
