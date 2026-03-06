fn main() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SW_HIDE;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow;

        unsafe {
            let console_window = GetConsoleWindow();
            if console_window != 0 {
                ShowWindow(console_window, SW_HIDE);
            }
        }
    }

    onvif_viewer_lib::run();
}
