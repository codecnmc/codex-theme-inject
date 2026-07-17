pub fn show_info(title: &str, message: &str) {
    show(title, message, false);
}

pub fn show_error(title: &str, message: &str) {
    show(title, message, true);
}

#[cfg(windows)]
fn show(title: &str, message: &str, error: bool) {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW,
    };
    use windows::core::PCWSTR;

    let title = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let message = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK
                | if error {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                },
        );
    }
}

#[cfg(not(windows))]
fn show(title: &str, message: &str, _error: bool) {
    eprintln!("{title}: {message}");
}
