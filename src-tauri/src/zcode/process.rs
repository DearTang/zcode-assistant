//! ZCode 进程控制：kill / 重启（参考 zcode-account-switcher）
use crate::zcode::paths;
use std::process::Command;
use std::time::Duration;

/// 强制结束 ZCode.exe，最多等待 ~8s 退出
pub fn kill_zcode() -> bool {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "ZCode.exe"])
            .output();
        for _ in 0..16 {
            if !paths::is_zcode_running() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        !paths::is_zcode_running()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// 后台启动 ZCode（detached，不阻塞）
pub fn launch_zcode() -> Option<()> {
    let exe = paths::find_zcode_exe()?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        Command::new(&exe)
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .ok()?;
    }
    #[cfg(not(windows))]
    {
        Command::new(&exe).spawn().ok()?;
    }
    Some(())
}

/// kill 后重启
pub fn restart() -> bool {
    let killed = kill_zcode();
    if !killed {
        return false;
    }
    launch_zcode().is_some()
}
