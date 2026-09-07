//! 开机自启：读写 HKCU\Software\Microsoft\Windows\CurrentVersion\Run。
//!
//! 不引入 tauri-plugin-autostart——为一个布尔开关新增整条依赖链不值得，
//! 且发布版要跑离线镜像拉包。这里用已有的 windows crate 直接落注册表，
//! 卸载或关闭开关时把值一并删掉，不留残余。

/// 注册表值名，与 NSIS productName 保持一致，便于用户在任务管理器「启动」页里认出
#[cfg(target_os = "windows")]
const RUN_VALUE: &str = "Sidebar Memo";
#[cfg(target_os = "windows")]
const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
/// RegDeleteValueW 对不存在的值返回 ERROR_FILE_NOT_FOUND
#[cfg(target_os = "windows")]
const ERROR_FILE_NOT_FOUND: u32 = 2;

#[cfg(target_os = "windows")]
fn widens(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 开启/关闭开机自启。失败时返回 Err，调用方不落盘，保证设置里的开关状态和注册表一致。
#[cfg(target_os = "windows")]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, KEY_WOW64_64KEY, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows_core::PCWSTR;

    let exe = std::env::current_exe().map_err(|e| format!("无法取得程序路径: {}", e))?;
    let subkey = widens(RUN_SUBKEY);
    let value = widens(RUN_VALUE);

    unsafe {
        let mut hkey = HKEY::default();
        // 用 Create 而不是 Open：精简系统上 Run 项可能不存在，Open 会直接失败
        let opened = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            PCWSTR(std::ptr::null::<u16>()),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            None,
            &mut hkey,
            None,
        );
        if opened.is_err() {
            return Err(format!("打开注册表启动项失败 (code {})", opened.0));
        }

        let applied = if enabled {
            // 安装路径常含空格（Program Files\Sidebar Memo），不引号包起来别的程序会截断
            let data = widens(&format!("\"{}\"", exe.display()));
            let bytes =
                std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
            RegSetValueExW(hkey, PCWSTR(value.as_ptr()), 0, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(hkey, PCWSTR(value.as_ptr()))
        };
        let _ = RegCloseKey(hkey);

        if applied.is_ok() {
            return Ok(());
        }
        // 关闭时值本来就没有（从未开启过）——目标状态已达成，不算失败
        if !enabled && applied.0 == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        Err(format!("写入注册表启动项失败 (code {})", applied.0))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("当前平台不支持开机自启".into())
}
