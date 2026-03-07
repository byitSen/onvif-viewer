#![cfg_attr(all(target_os = "windows"), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_MINIMIZE};

        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
            let console_window: HWND =
                windows_sys::Win32::UI::WindowsAndMessaging::GetConsoleWindow();
            if console_window != 0 {
                ShowWindow(console_window, SW_MINIMIZE);
            }
        }
    }

    onvif_viewer_lib::run();
}
