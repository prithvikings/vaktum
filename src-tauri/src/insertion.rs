#[cfg(windows)]
use std::{thread, time::Duration};

#[cfg(windows)]
use clipboard_win::{get_clipboard, set_clipboard_string};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentProcessId,
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, KEYBDINPUT, KEYEVENTF_KEYUP, INPUT_KEYBOARD, VK_CONTROL,
            VK_V,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
            SetForegroundWindow, ShowWindow, SW_RESTORE,
        },
    },
};

#[cfg(windows)]
pub fn capture_target_window() -> Result<i64, String> {
    unsafe {
        let hwnd = GetForegroundWindow();

        if hwnd.is_null() {
            return Err("No foreground application is available".to_owned());
        }

        if is_current_process_window(hwnd) {
            return Err("Vaktum is currently focused; no external target application was captured".to_owned());
        }

        Ok(hwnd as i64)
    }
}

#[cfg(not(windows))]
pub fn capture_target_window() -> Result<i64, String> {
    Err("Focused-app text insertion is only supported on Windows".to_owned())
}

pub fn insert_text(target_window: i64, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("No transcript available for insertion".to_owned());
    }

    #[cfg(windows)]
    {
        insert_text_windows(target_window, text)
    }

    #[cfg(not(windows))]
    {
        let _ = target_window;
        let _ = text;
        Err("Focused-app text insertion is only supported on Windows".to_owned())
    }
}

#[cfg(windows)]
fn insert_text_windows(target_window: i64, text: &str) -> Result<(), String> {
    let hwnd = target_window as HWND;

    unsafe {
        validate_target_window(hwnd)?;

        let previous_clipboard: Option<String> =
            get_clipboard(clipboard_win::formats::Unicode).ok();

        set_clipboard_string(text)
            .map_err(|error| format!("Clipboard operation failed: {error}"))?;

        let clipboard_sequence = clipboard_win::raw::seq_num();

        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }

        if SetForegroundWindow(hwnd) == 0 {
            restore_clipboard(previous_clipboard.as_deref(), clipboard_sequence);
            return Err("Could not activate the previously focused application".to_owned());
        }

        thread::sleep(Duration::from_millis(50));

        if GetForegroundWindow() != hwnd {
            restore_clipboard(previous_clipboard.as_deref(), clipboard_sequence);
            return Err("Could not activate the previously focused application".to_owned());
        }

        if let Err(error) = send_paste() {
            restore_clipboard(previous_clipboard.as_deref(), clipboard_sequence);
            return Err(error);
        }

        // Give the target application a short opportunity to consume Ctrl+V before
        // restoring text-only clipboard contents. If the user changed the clipboard
        // during this window, leave their new clipboard contents untouched.
        thread::sleep(Duration::from_millis(100));
        restore_clipboard(previous_clipboard.as_deref(), clipboard_sequence);

        Ok(())
    }
}

#[cfg(windows)]
unsafe fn validate_target_window(hwnd: HWND) -> Result<(), String> {
    if hwnd.is_null() || IsWindow(hwnd) == 0 {
        return Err("The previously focused application window is no longer available".to_owned());
    }

    if is_current_process_window(hwnd) {
        return Err("Refusing to insert text into Vaktum".to_owned());
    }

    Ok(())
}

#[cfg(windows)]
unsafe fn is_current_process_window(hwnd: HWND) -> bool {
    let mut process_id = 0;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    process_id != 0 && process_id == GetCurrentProcessId()
}

#[cfg(windows)]
unsafe fn send_paste() -> Result<(), String> {
    let inputs = [
        keyboard_input(VK_CONTROL, 0),
        keyboard_input(VK_V, 0),
        keyboard_input(VK_V, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];

    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<INPUT>() as i32,
    );

    if sent != inputs.len() as u32 {
        return Err("Paste/input operation failed".to_owned());
    }

    Ok(())
}

#[cfg(windows)]
const fn keyboard_input(key: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn restore_clipboard(previous: Option<&str>, sequence_after_insert: Option<std::num::NonZeroU32>) {
    let Some(previous) = previous else {
        return;
    };

    let Some(expected_sequence) = sequence_after_insert else {
        return;
    };

    if clipboard_win::raw::seq_num() != Some(expected_sequence) {
        return;
    }

    let _ = set_clipboard_string(previous);
}

#[cfg(test)]
mod tests {
    use super::insert_text;

    #[test]
    fn rejects_empty_text() {
        let error = insert_text(0, "").expect_err("empty text should be rejected");
        assert_eq!(error, "No transcript available for insertion");
    }

    #[test]
    fn rejects_whitespace_only_text() {
        let error = insert_text(0, "   \n\t").expect_err("whitespace should be rejected");
        assert_eq!(error, "No transcript available for insertion");
    }
}
