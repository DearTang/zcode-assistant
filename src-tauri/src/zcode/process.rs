//! ZCode 进程控制：kill / 重启（参考 zcode-account-switcher）
use crate::zcode::{config_file, paths};
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

/// 给 spawn 的 console 子进程统一加 CREATE_NO_WINDOW：
/// 本应用是 GUI 进程（无控制台），Windows 上默认会为每个 console 子进程
/// （cmd/taskkill/tasklist/wmic/powershell）弹出一个黑色终端窗口，
/// kill/启动/轮询连环 spawn 时用户会看到一串终端闪现。
#[cfg(windows)]
pub(crate) fn no_window(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}
#[cfg(not(windows))]
pub(crate) fn no_window(cmd: &mut Command) -> &mut Command {
    cmd
}

/// 计算 provider/model 在 ZCode Ctrl+M 模型菜单中的位置（0=第一项，需 DOWN 的次数）。
///
/// 菜单真实结构（源码调研，app.asar renderer `E6e/Mq/S6e/m6e`，Radix DropdownMenu）：
/// - 分组下拉：每个分组一个组头（**不占键盘导航位**）+ 组内模型项（每项一个模型，可直接选中）；
/// - 智谱 family（zai / bigmodel）折叠为**一组**，组内只显示该 family「当前选中」
///   套餐/供应商的模型（Start/Individual/Team/API Key 通过组头下拉切换，键盘不可达）；
/// - 其余供应商（builtin:zapi、自定义）各自一组；
/// - 分组顺序 = zai 系(权重0-2) < bigmodel 系(3-5) < zapi(6) < 自定义(200)，
///   同权重按 config.json 出现顺序；setting.providerFamilyDomain 之外的 family 不显示；
/// - 底部有「管理模型」等 footer 项（位于最后，正常目标不会越过）。
///
/// 因此位置 = 目标项之前所有分组的模型数总和 + 组内索引。
pub fn model_menu_position(
    provider_key: &str,
    model_key: Option<&str>,
) -> Result<usize, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let setting = config_file::read_setting().map_err(|e| e.to_string())?;
    let providers = config
        .get("provider")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "config.json 无 provider 对象".to_string())?;

    // 当前 family domain（glm 模式按 bigmodel 处理）；无 domain 时不过滤
    let domain: Option<String> = setting
        .get("providerFamilyDomain")
        .and_then(|v| v.as_str())
        .map(|s| if s == "glm" { "bigmodel".to_string() } else { s.to_string() });

    let model_names = |prov: &Value| -> Vec<String> {
        prov.get("models")
            .and_then(|m| m.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    let usable = |prov: &Value, require_enabled: bool| -> bool {
        if prov.get("systemDisabledReason").is_some() {
            return false;
        }
        if require_enabled && prov.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
            return false;
        }
        true
    };

    // 按 (菜单权重, config 出现顺序) 排序——sort_by_key 为稳定排序，次序即 config 顺序
    let mut sorted: Vec<(&String, &Value)> = providers.iter().collect();
    sorted.sort_by_key(|(k, _)| provider_weight(k));

    // 1) family 组（菜单里智谱 family 折叠为一组）：组内只显示「当前选中」
    //    套餐的模型；选中项取 setting 的 familySelectedKeys，缺省/无效取权重最小成员
    let mut family_group: Option<(String, String, Vec<String>)> = None;
    if let Some(dom) = domain.as_deref() {
        let members: Vec<(&String, &Value)> = sorted
            .iter()
            .filter(|(k, v)| family_of(k) == Some(dom) && usable(v, true))
            .cloned()
            .collect();
        if let Some(first) = members.first() {
            let selected = config_file::current_selected(&setting, dom)
                .as_deref()
                .map(config_file::selected_to_provider);
            let chosen = members
                .iter()
                .find(|(k, _)| Some(k.as_str()) == selected.as_deref())
                .unwrap_or(first);
            let ms = model_names(chosen.1);
            if !ms.is_empty() {
                family_group = Some((dom.to_string(), (*chosen.0).clone(), ms));
            }
        }
    }

    // 2) 独立组：非 family 的 builtin（zapi 等）+ 自定义供应商（需 apiKey 与模型）。
    //    family 成员不单独成组（折叠/被 domain 过滤）
    let mut single_groups: Vec<(String, Vec<String>)> = Vec::new();
    for (k, v) in &sorted {
        match family_of(k) {
            Some(fam) => {
                // 只允许与 domain 相同的 family；其组已在上方折叠
                let _ = fam;
                continue;
            }
            None => {
                if k.starts_with("builtin:") {
                    // zapi 等非 family 内置
                    if usable(v, true) {
                        let ms = model_names(v);
                        if !ms.is_empty() {
                            single_groups.push(((*k).clone(), ms));
                        }
                    }
                } else {
                    // 自定义供应商：需要 apiKey 非空
                    let has_key = v
                        .get("options")
                        .and_then(|o| o.get("apiKey"))
                        .and_then(|a| a.as_str())
                        .map(|s| !s.trim().is_empty())
                        .unwrap_or(false);
                    if has_key && usable(v, false) {
                        let ms = model_names(v);
                        if !ms.is_empty() {
                            single_groups.push(((*k).clone(), ms));
                        }
                    }
                }
            }
        }
    }

    // 组内索引（未指定模型取组内第一项）；找不到报错而非静默取 0
    let in_group_idx = |items: &[String]| -> Result<usize, String> {
        match model_key.filter(|m| !m.is_empty()) {
            Some(m) => items
                .iter()
                .position(|x| x == m)
                .ok_or_else(|| format!("「{provider_key}」模型列表中未找到「{m}」")),
            None => Ok(0),
        }
    };

    let target_family = if provider_key.starts_with("builtin:") {
        family_of(provider_key)
    } else {
        None
    };
    let family_len = family_group.as_ref().map(|(_, _, ms)| ms.len()).unwrap_or(0);

    match target_family {
        // 套餐目标：只能落在 family 组，且组当前显示的正是目标套餐
        Some(fam) => match &family_group {
            Some((dom, gid, ms)) if *dom == fam && gid == provider_key => {
                Ok(in_group_idx(ms)?)
            }
            Some((dom, gid, _)) if *dom == fam => Err(format!(
                "菜单当前 {fam} 组显示的是「{gid}」的模型（目标 {provider_key} 未选中），\
                 请先在 ZCode 模型菜单顶部把该组切到目标套餐"
            )),
            _ => Err(format!(
                "套餐「{provider_key}」不在当前模型菜单（未启用/未授权，或与当前 providerFamilyDomain 不符）"
            )),
        },
        // 自定义 / zapi 目标：family 组在前，独立组按序排列
        None => {
            let mut pos = family_len;
            for (gid, ms) in &single_groups {
                if gid == provider_key {
                    return Ok(pos + in_group_idx(ms)?);
                }
                pos += ms.len();
            }
            Err(format!(
                "未在模型菜单中找到供应商「{provider_key}」（可能未启用或缺少 API Key）"
            ))
        }
    }
}

/// builtin 供应商所属智谱 family（zai / bigmodel）；其余返回 None
fn family_of(provider_key: &str) -> Option<&'static str> {
    match provider_key.strip_prefix("builtin:")? {
        "zai" | "zai-coding-plan" | "zai-start-plan" => Some("zai"),
        "bigmodel" | "bigmodel-coding-plan" | "bigmodel-start-plan" => Some("bigmodel"),
        _ => None,
    }
}

/// ZCode 模型菜单的分组排序权重（renderer `i6e` 映射；越小越靠前，自定义=200）
fn provider_weight(provider_key: &str) -> i32 {
    match provider_key {
        "builtin:zai-start-plan" => 0,
        "builtin:zai-coding-plan" => 1,
        "builtin:zai" => 2,
        "builtin:bigmodel-start-plan" => 3,
        "builtin:bigmodel-coding-plan" => 4,
        "builtin:bigmodel" => 5,
        "builtin:zapi" => 6,
        _ => 200,
    }
}

/// 只读 dry-run：计算真实环境下各目标在 Ctrl+M 菜单中的位置（不执行切换）。
/// 手动跑：`cargo test -- --ignored menu_position_dry`
#[cfg(test)]
mod tests {
    use super::model_menu_position;

    #[test]
    #[ignore]
    fn menu_position_dry() {
        for (p, m) in [
            ("builtin:bigmodel-coding-plan", Some("GLM-5-Turbo")),
            ("builtin:bigmodel-coding-plan", Some("GLM-5.3")),
            ("builtin:bigmodel-coding-plan", None),
        ] {
            match model_menu_position(p, m) {
                Ok(pos) => println!("{p}/{m:?} -> DOWN {pos}"),
                Err(e) => println!("{p}/{m:?} -> ERR {e}"),
            }
        }
    }
}

/// 强制结束 ZCode.exe，最多等待 ~8s 退出
pub fn kill_zcode() -> bool {
    #[cfg(windows)]
    {
        let _ = no_window(&mut Command::new("taskkill"))
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

/// 后台启动 ZCode（detached，不阻塞）。返回 exe 路径供调用方报错。
/// 用 `cmd /c start` 包一层，比直接 spawn + DETACHED_PROCESS 更可靠地脱离父进程，
/// 避免 zcode-assistant 退出/重启时连带把刚拉起的 ZCode 也带走。
pub fn launch_zcode() -> Result<String, String> {
    let exe = paths::find_zcode_exe()
        .ok_or_else(|| "未找到 ZCode.exe（可在设置页手动指定路径）".to_string())?;
    #[cfg(windows)]
    {
        // start "" "<exe>" —— 空标题 + 独立进程启动
        let status = no_window(&mut Command::new("cmd"))
            .args(["/C", "start", "", &exe])
            .spawn()
            .map_err(|e| format!("启动失败: {e}"))?;
        // 不等待子进程；spawn 成功即认为已派发
        let _ = status;
    }
    #[cfg(not(windows))]
    {
        Command::new(&exe)
            .spawn()
            .map_err(|e| format!("启动失败: {e}"))?;
    }
    Ok(exe)
}

/// kill 后重启。返回 Ok(()) 或具体失败原因（供前端提示）。
pub fn restart() -> Result<(), String> {
    if !kill_zcode() {
        return Err("ZCode 未能关闭（taskkill 失败或超时）".into());
    }
    // 关闭后给系统一点时间释放文件锁/端口
    std::thread::sleep(Duration::from_millis(500));
    match launch_zcode() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("ZCode 已关闭但重新启动失败：{e}")),
    }
}

/// 轮询等待 ZCode 主窗口出现（重启后调用，Electron 启动需数秒）。
/// 单次 PowerShell 内部循环，避免反复 spawn；超时返回 false。
/// 现自动切换已改直写 cli db 的 model_selection（免 UI 模拟），暂无调用方，保留备用。
#[allow(dead_code)]
pub fn wait_zcode_main_window(timeout_secs: u64) -> bool {
    #[cfg(windows)]
    {
        let ps = format!(
            r#"
$deadline = (Get-Date).AddSeconds({timeout_secs})
while ((Get-Date) -lt $deadline) {{
  $p = Get-Process -Name 'ZCode' -ErrorAction SilentlyContinue |
       Where-Object {{ $_.MainWindowHandle -ne [IntPtr]::Zero }} | Select-Object -First 1
  if ($p) {{ exit 0 }}
  Start-Sleep -Milliseconds 800
}}
exit 2
"#
        );
        matches!(
            no_window(&mut Command::new("powershell.exe"))
                .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
                .output(),
            Ok(o) if o.status.code() == Some(0)
        )
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 重启 ZCode 并在窗口就绪后键盘模拟选中目标模型（会抢前台焦点，仅备选方案）。
/// 历史路径：曾用于「切换后重启」模式下带模型级目标的场景；现自动切换改为直写
/// cli db 的 runtime/model_selection（会话恢复时还原模型），不再需要重启后模拟，
/// 保留作为兜底工具。
#[allow(dead_code)]
pub fn restart_then_select_model(
    provider_key: &str,
    model_key: &str,
) -> Result<(), String> {
    restart()?;
    if !wait_zcode_main_window(40) {
        return Err(
            "ZCode 已重启，但等待其窗口就绪超时，未能自动选择模型（请手动 Ctrl+M 选择）".into(),
        );
    }
    // 主窗口出现后再留几秒给渲染进程就绪（菜单键盘导航需要 renderer 已挂载）
    std::thread::sleep(Duration::from_secs(4));
    let pos = model_menu_position(provider_key, Some(model_key))?;
    switch_model_window(pos)
}

/// 触发 ZCode 执行「Developer: Reload Window」（PowerShell 键盘模拟）。
/// 精确找到 ZCode.exe 主窗口 → 激活 → Ctrl+Shift+P → 输入 "reload window" → 回车。
/// 比 kill+relaunch 温和：只重载窗口、重读配置、保留打开的文件，不杀进程。
/// 失败返回 false（调用方可回退到 restart 或提示用户手动）。
pub fn reload_window() -> bool {
    #[cfg(windows)]
    {
        let ps = r#"
$ErrorActionPreference = 'Stop'
$proc = Get-Process -Name 'ZCode' -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
if (-not $proc) { exit 2 }
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class WinApi {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@
[void][WinApi]::ShowWindowAsync($proc.MainWindowHandle, 9)
Start-Sleep -Milliseconds 120
[void][WinApi]::SetForegroundWindow($proc.MainWindowHandle)
$fg = $false
for ($i=0; $i -lt 20; $i++) {
  Start-Sleep -Milliseconds 100
  if ([WinApi]::GetForegroundWindow() -eq $proc.MainWindowHandle) { $fg = $true; break }
}
if (-not $fg) { exit 4 }
$shell = New-Object -ComObject WScript.Shell
# F1 打开命令面板（VSCode 系标准，等同 Ctrl+Shift+P；不带 Shift，避免修饰键残留导致后续字符变大写/吞空格）
$shell.SendKeys('{F1}')
Start-Sleep -Milliseconds 700
$shell.SendKeys('reload window')
Start-Sleep -Milliseconds 500
$shell.SendKeys('{ENTER}')
exit 0
"#;
        match no_window(&mut Command::new("powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", ps])
            .output()
        {
            Ok(o) => o.status.code() == Some(0),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 通过键盘模拟切换 ZCode 当前会话的模型（免重启）。
///
/// Ctrl+M 菜单是**纯列表无输入框**，无法用文本过滤。改用按位置 DOWN 导航：
/// 菜单顺序 = [当前套餐模型 N 个] + [供应商列表]，第一项默认高亮。
/// down_count = 目标项在菜单中的位置（0=第一项，需 DOWN 0 次）。
///
/// 流程：激活 ZCode（Alt trick）→ Ctrl+M 打开菜单 → 循环 DOWN down_count 次 → Enter 选中
/// keybd_event 发键对 Electron 友好；结束恢复 NumLock + CapsLock。
pub fn switch_model_window(down_count: usize) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::io::Write;

        // 1) 把 C# 代码写到临时文件（避开 PowerShell here-string 解析陷阱）
        let cs_code = r#"using System;
using System.Runtime.InteropServices;

public static class ZInput {
  [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] static extern bool ShowWindowAsync(IntPtr h, int n);
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] static extern short GetKeyState(int v);
  [DllImport("user32.dll")] static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);

  // Ctrl+M：用 keybd_event（对 Electron 友好）
  public static void SendCtrlM() {
    keybd_event(0x11, 0, 0, UIntPtr.Zero);   // VK_CONTROL down
    keybd_event(0x4D, 0, 0, UIntPtr.Zero);   // M down
    keybd_event(0x4D, 0, 2, UIntPtr.Zero);   // M up
    keybd_event(0x11, 0, 2, UIntPtr.Zero);   // VK_CONTROL up
  }
  // 单键（DOWN/ENTER）：用 keybd_event
  public static void SendVk(byte vk) {
    keybd_event(vk, 0, 0, UIntPtr.Zero);
    keybd_event(vk, 0, 2, UIntPtr.Zero);
  }

  public static bool GetNumLock() { return (GetKeyState(0x90) & 1) != 0; }
  public static bool GetCapsLock() { return (GetKeyState(0x14) & 1) != 0; }
  public static void RestoreToggle(int vk, bool wantOn) {
    bool cur = (GetKeyState(vk) & 1) != 0;
    if (cur != wantOn) {
      keybd_event((byte)vk, 0x45, 1, UIntPtr.Zero);
      keybd_event((byte)vk, 0x45, 3, UIntPtr.Zero);
    }
  }

  // downCount：菜单打开后向下导航的次数（0=选第一项）
  public static int Do(int downCount) {
    var proc = System.Diagnostics.Process.GetProcessesByName("ZCode");
    System.Diagnostics.Process p = null;
    foreach (var x in proc) { if (x.MainWindowHandle != IntPtr.Zero) { p = x; break; } }
    if (p == null) return 2;

    ShowWindowAsync(p.MainWindowHandle, 9); // SW_RESTORE
    System.Threading.Thread.Sleep(300);

    // Alt trick：按 Alt 再松开，重置 Windows 前台锁（否则 SetForegroundWindow 会被静默拒绝）
    keybd_event(0x12, 0, 0, UIntPtr.Zero);   // VK_MENU down
    keybd_event(0x12, 0, 2, UIntPtr.Zero);   // VK_MENU up
    System.Threading.Thread.Sleep(100);

    SetForegroundWindow(p.MainWindowHandle);
    BringWindowToTop(p.MainWindowHandle);

    bool fg = false;
    for (int i = 0; i < 30; i++) {
      System.Threading.Thread.Sleep(100);
      if (GetForegroundWindow() == p.MainWindowHandle) { fg = true; break; }
    }
    if (!fg) return 4;

    // 等 Electron 渲染进程准备好接收键盘事件
    System.Threading.Thread.Sleep(500);

    // Ctrl+M 打开模型选择菜单
    SendCtrlM();
    System.Threading.Thread.Sleep(800);

    // 循环 DOWN 导航到目标位置（菜单是纯列表无输入框，靠位置定位）
    for (int i = 0; i < downCount; i++) {
      SendVk(0x28); // VK_DOWN
      System.Threading.Thread.Sleep(80);
    }
    System.Threading.Thread.Sleep(200);

    // Enter 选中
    SendVk(0x0D); // VK_RETURN
    System.Threading.Thread.Sleep(200);
    return 0;
  }
}
"#;
        let temp_dir = std::env::temp_dir();
        let cs_path = temp_dir.join("zcode_assistant_zinput.cs");
        let mut f = std::fs::File::create(&cs_path)
            .map_err(|e| format!("无法创建临时 C# 文件: {e}"))?;
        f.write_all(cs_code.as_bytes())
            .map_err(|e| format!("无法写入临时 C# 文件: {e}"))?;
        drop(f);

        // 2) cs 路径转义（PowerShell 单引号字符串）
        let cs_path_str = cs_path
            .to_str()
            .ok_or_else(|| "临时路径含非法字符".to_string())?
            .replace('\'', "''");

        // 3) PowerShell：Add-Type -Path 编译 C# → 调用 Do(down_count) → 恢复 NumLock/CapsLock
        let ps = format!(
            r#"
$ErrorActionPreference = 'Stop'
Add-Type -Path '{cs_path_str}'
$nlWas = [bool]([ZInput]::GetNumLock())
$clWas = [bool]([ZInput]::GetCapsLock())
$rc = [ZInput]::Do({down_count})
[ZInput]::RestoreToggle(0x90, $nlWas)
[ZInput]::RestoreToggle(0x14, $clWas)
exit $rc
"#
        );
        let out = no_window(&mut Command::new("powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
            .output()
            .map_err(|e| format!("启动键盘模拟失败: {e}"))?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        match out.status.code() {
            Some(0) => Ok(()),
            Some(2) => Err("未找到 ZCode 主窗口（ZCode 是否在运行？）".into()),
            Some(4) => Err("无法激活 ZCode 窗口（可能被其他窗口遮挡/全屏）".into()),
            Some(c) => {
                let detail = stderr.trim();
                if detail.is_empty() {
                    Err(format!("键盘模拟异常退出码: {c}"))
                } else {
                    Err(format!("键盘模拟异常（码 {c}）：{detail}"))
                }
            }
            None => Err("键盘模拟进程被终止".into()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = down_count;
        Err("当前平台不支持键盘模拟切换模型".into())
    }
}
