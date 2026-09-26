use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplicationKind {
    #[serde(rename = "vscode")]
    VsCode,
    Notepad,
    Browser,
    Terminal,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DictationContext {
    pub application: ApplicationKind,
    pub process_name: String,
    pub captured_at: u64,
}

pub fn detect(target_window: i64) -> DictationContext {
    let process_name = resolve_process_name(target_window).unwrap_or_default();
    let application = classify_process_name(&process_name);

    DictationContext {
        application,
        process_name,
        captured_at: now_unix_timestamp(),
    }
}

pub fn classify_process_name(process_name: &str) -> ApplicationKind {
    let executable = process_name
        .rsplit_once(['\\\\', '/'])
        .map(|(_, name)| name)
        .unwrap_or(process_name)
        .trim();

    if executable.is_empty() {
        return ApplicationKind::Unknown;
    }

    match executable.to_ascii_lowercase().as_str() {
        "code.exe" | "code-insiders.exe" => ApplicationKind::VsCode,
        "notepad.exe" => ApplicationKind::Notepad,
        "chrome.exe" | "msedge.exe" | "firefox.exe" => ApplicationKind::Browser,
        "windowsterminal.exe" | "wt.exe" | "cmd.exe" | "powershell.exe" | "pwsh.exe" => {
            ApplicationKind::Terminal
        }
        _ => ApplicationKind::Unknown,
    }
}

fn now_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(windows)]
fn resolve_process_name(target_window: i64) -> Option<String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND},
        System::Threading::{
            GetCurrentProcessId, GetWindowThreadProcessId, OpenProcess,
            QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::IsWindow,
    };

    let hwnd = target_window as HWND;

    unsafe {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return None;
        }

        let mut process_id = 0_u32;
        if GetWindowThreadProcessId(hwnd, &mut process_id) == 0 || process_id == 0 {
            return None;
        }

        if process_id == GetCurrentProcessId() {
            return None;
        }

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return None;
        }

        let mut buffer = [0_u16; 32_768];
        let mut length = buffer.len() as u32;

        let result =
            QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);

        CloseHandle(process);

        if result == 0 || length == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit_once(['\\\\', '/'])
            .map(|(_, name)| name.to_owned())
            .or_else(|| Some(path))
    }
}

#[cfg(not(windows))]
fn resolve_process_name(_target_window: i64) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::{classify_process_name, ApplicationKind};

    #[test]
    fn classifies_vscode() {
        assert_eq!(classify_process_name("Code.exe"), ApplicationKind::VsCode);
        assert_eq!(
            classify_process_name("C:\Program Files\Microsoft VS Code\Code.exe"),
            ApplicationKind::VsCode
        );
        assert_eq!(
            classify_process_name("code-insiders.exe"),
            ApplicationKind::VsCode
        );
    }

    #[test]
    fn classifies_notepad() {
        assert_eq!(classify_process_name("notepad.exe"), ApplicationKind::Notepad);
    }

    #[test]
    fn classifies_common_browsers() {
        assert_eq!(classify_process_name("chrome.exe"), ApplicationKind::Browser);
        assert_eq!(classify_process_name("msedge.exe"), ApplicationKind::Browser);
        assert_eq!(classify_process_name("firefox.exe"), ApplicationKind::Browser);
    }

    #[test]
    fn classifies_windows_terminals() {
        assert_eq!(
            classify_process_name("WindowsTerminal.exe"),
            ApplicationKind::Terminal
        );
        assert_eq!(classify_process_name("wt.exe"), ApplicationKind::Terminal);
        assert_eq!(classify_process_name("cmd.exe"), ApplicationKind::Terminal);
        assert_eq!(classify_process_name("powershell.exe"), ApplicationKind::Terminal);
        assert_eq!(classify_process_name("pwsh.exe"), ApplicationKind::Terminal);
    }

    #[test]
    fn unsupported_process_is_unknown() {
        assert_eq!(
            classify_process_name("explorer.exe"),
            ApplicationKind::Unknown
        );
    }

    #[test]
    fn empty_process_name_is_unknown() {
        assert_eq!(classify_process_name(""), ApplicationKind::Unknown);
        assert_eq!(classify_process_name("   "), ApplicationKind::Unknown);
    }

    #[test]
    fn classification_does_not_modify_process_name() {
        let process_name = "C:\Program Files\Google\Chrome\chrome.exe";
        assert_eq!(classify_process_name(process_name), ApplicationKind::Browser);
        assert_eq!(process_name, "C:\Program Files\Google\Chrome\chrome.exe");
    }

    #[test]
    fn classification_does_not_panic_for_invalid_names() {
        let names = ["", " ", ".", ".exe", "not-an-executable", "???"];
        for name in names {
            let _ = classify_process_name(name);
        }
    }
}
