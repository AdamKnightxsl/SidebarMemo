#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::Threading::{CreateMutexW, CreateEventW, OpenEventW, SetEvent, SYNCHRONIZATION_ACCESS_RIGHTS};
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_core::PCWSTR;

        // 命名互斥量：用于检测是否已有实例运行
        let mutex_name: Vec<u16> = "Global\\SidebarMemoSingleInstance\0"
            .encode_utf16()
            .collect();
        // 命名事件：用于通知已有实例显示窗口
        let event_name: Vec<u16> = "Global\\SidebarMemoShowEvent\0"
            .encode_utf16()
            .collect();

        unsafe {
            let _mutex = CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr()));
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // 已有实例运行，尝试打开事件并通知它显示窗口
                let access = SYNCHRONIZATION_ACCESS_RIGHTS(0x0002); // EVENT_MODIFY_STATE
                if let Ok(event) = OpenEventW(access, false, PCWSTR(event_name.as_ptr())) {
                    let _ = SetEvent(event);
                    let _ = windows::Win32::Foundation::CloseHandle(event);
                }
                std::process::exit(0);
            }
            std::mem::forget(_mutex);

            // 第一个实例：创建手动重置事件，初始状态为未触发
            // HANDLE 是 Copy 类型，进程退出时 OS 自动回收
            let _ = CreateEventW(None, true, false, PCWSTR(event_name.as_ptr()));
        }
    }

    sidebar_memo::run()
}
