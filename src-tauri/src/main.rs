fn main() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetConsoleWindow, ShowWindow, SW_HIDE};

        unsafe {
            let console = GetConsoleWindow();
            if console != 0 {
                ShowWindow(console, SW_HIDE);
            }
        }
    }

    onvif_viewer_lib::run();
}
