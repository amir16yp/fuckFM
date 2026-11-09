//! Fatal error / panic reporting — MessageBox on Windows, stderr elsewhere.

use std::fmt::Write as _;

/// Show a blocking error to the user, then return (caller exits).
pub fn show_error(product: &str, err: &dyn std::fmt::Debug) {
    let mut body = String::new();
    let _ = write!(&mut body, "{err:?}");
    show_error_text(product, &body);
}

pub fn show_error_text(product: &str, message: &str) {
    eprintln!("{product} error: {message}");
    #[cfg(windows)]
    {
        message_box(product, message);
    }
}

/// Install a panic hook that shows a MessageBox on Windows (useful when fuckwm is Shell).
pub fn install_panic_hook(product: &'static str) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut msg = String::new();
        if let Some(s) = info.payload().downcast_ref::<&str>() {
            let _ = write!(&mut msg, "{s}");
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            let _ = write!(&mut msg, "{s}");
        } else {
            let _ = write!(&mut msg, "unknown panic");
        }
        if let Some(loc) = info.location() {
            let _ = write!(&mut msg, "\n\n{}", loc);
        }
        show_error_text(product, &msg);
        default(info);
    }));
}

#[cfg(windows)]
fn message_box(title: &str, body: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    // Cap length so the dialog stays usable.
    let truncated = if body.len() > 1500 {
        format!("{}…", &body[..1500])
    } else {
        body.to_string()
    };
    let title_w = wide(title);
    let body_w = wide(&truncated);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}
