/**
 * 匿名版本级使用统计（参考 myshell，经 Umami Cloud 上报）。
 *
 * 设计（隐私优先）：
 *   - 一次性生成随机设备 ID 存 localStorage，**不**绑定任何个人信息——仅为让
 *     Umami 跨多次启动去重（统计「唯一安装」而非「启动次数」）。
 *   - 每个新版本首次启动发送一次匿名事件：{ device_id, app_version, os }。
 *     绝不发送 服务器地址 / 用户名 / API Key / 凭据 / 连接内容。
 *   - 同意模型：每版本询问一次。同意则记住偏好，后续版本静默上报（升级不再
 *     重复弹）；拒绝则不记住偏好，每个新版本再问一次。同意后若发送失败，下次
 *     启动静默重试（不再弹窗）直到成功。
 *
 * Umami Cloud endpoint：POST https://cloud.umami.is/api/send
 *   body: { type: "event", payload: { website, name, data: {...} } }
 *
 * website ID 按设计是公开的（Umami 模型——它只允许向你的面板发事件，不能读回数据）。
 *
 * ⚠️ 配置：在 https://cloud.umami.is 注册免费站点后，把下面的 UMAMI_WEBSITE_ID 换成
 * 你自己的站点 ID。留空时 reportVersion 直接跳过（无副作用）。本项目 csp 为 null，
 * webview 的 fetch 可直连 Umami，无需改 CSP。
 */

// --- 配置（来自你的 Umami Cloud 面板）---
// 与 myshell 共用同一 Umami 站点（同一站点下用 event name / app_version 区分）。
const UMAMI_ENDPOINT = "https://cloud.umami.is/api/send";
const UMAMI_WEBSITE_ID = "e3302fe3-b5fc-411f-8bc7-5948d3c923bb";

// --- localStorage 键（za.* 前缀）---
const KEY_DEVICE_ID = "za.deviceId";
const KEY_CONSENT = "za.statsConsent";
const KEY_VERSION = "za.statsVersion";

/** 生成或取回持久化的匿名设备 ID。 */
export function getDeviceId(): string {
  try {
    let id = localStorage.getItem(KEY_DEVICE_ID);
    if (!id) {
      // Crypto.randomUUID 在 WebView2（Chromium 92+）可用。
      id = crypto.randomUUID();
      localStorage.setItem(KEY_DEVICE_ID, id);
    }
    return id;
  } catch {
    // localStorage 不可用——本次会话用一次性 ID。
    return "unknown-device";
  }
}

/** 用户此前是否已同意匿名使用统计？ */
export function hasStatsConsent(): boolean {
  try {
    return localStorage.getItem(KEY_CONSENT) === "agreed";
  } catch {
    return false;
  }
}

/**
 * 当前版本是否已被处理（已询问并决定）？
 *
 * KEY_VERSION 记录「用户已为该版本被询问过并做了决定（同意或拒绝）」，故同一版本
 * 后续启动不再弹。只有版本号变化才会重新触发弹窗。见 {@link markVersionHandled}。
 */
export function isVersionReported(version: string): boolean {
  try {
    return localStorage.getItem(KEY_VERSION) === version;
  } catch {
    return false;
  }
}

/**
 * 把某版本标记为「已处理」——用户已被询问并做了决定，故同版本后续启动不再弹。
 * agree 和 decline 回调都会调用，保证无论用户选择（或 agree 路径网络失败）都会盖戳。
 * 只有版本号变化才会清掉它并重新弹。
 *
 * reportVersion 在发送成功后也会写这个键（agree 路径已写过则冗余但无害，且让仅
 * 上报的调用方保持一致）。
 */
export function markVersionHandled(version: string): void {
  try {
    localStorage.setItem(KEY_VERSION, version);
  } catch {
    /* best-effort */
  }
}

/**
 * 为当前版本发送一次匿名事件。fire-and-forget——网络失败静默忽略（尽力而为的
 * 遥测，不是关键功能）。永不抛错。
 */
export async function reportVersion(version: string, os: string): Promise<void> {
  // 未配置 website ID 时直接跳过（功能开关）。
  if (!UMAMI_WEBSITE_ID) return;

  const deviceId = getDeviceId();

  const body = {
    type: "event" as const,
    payload: {
      website: UMAMI_WEBSITE_ID,
      name: `app_launch_v${version}`,
      // 自定义属性——在 Umami 面板可查看并分组。
      data: {
        device_id: deviceId,
        app_version: version,
        os,
      },
    },
  };

  try {
    await fetch(UMAMI_ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    // 标记该版本已上报，下次启动不再发。
    try {
      localStorage.setItem(KEY_VERSION, version);
    } catch {
      /* best-effort */
    }
  } catch {
    // 网络失败——不标记为已上报，下次启动重试。静默：遥测绝不能打扰用户。
  }
}

/**
 * 记录用户的同意决定。「agreed」会被持久化，后续版本静默上报。拒绝不被持久化——
 * 每个新版本都会重新询问。
 */
export function setStatsConsent(agreed: boolean): void {
  try {
    if (agreed) {
      localStorage.setItem(KEY_CONSENT, "agreed");
    } else {
      // 不存「declined」——缺省即「下版本再问」。
      localStorage.removeItem(KEY_CONSENT);
    }
  } catch {
    /* best-effort */
  }
}

/**
 * 判断当前版本是否需要上报，以及是否需要先征求同意。在应用启动时调用。
 *
 * 同意模型（问一次）：
 *   - 首次启动（无同意记录）：弹窗询问。
 *   - 用户此前同意：后续所有版本静默上报，升级不再重弹。也覆盖失败重试：若上次
 *     发送失败（网络/CSP），版本戳没写，下次启动返回 {shouldReport:true,
 *     hasConsent:true} 静默重试。
 *   - 用户拒绝：同意不被记住，故每个新版本重新弹（版本升级是值得再问的新事件）。
 *
 * 「已处理」（isVersionReported）表示该版本已成功上报（或被明确拒绝），故什么也不做。
 *
 * 返回：
 *   - { shouldReport: true,  hasConsent: true  } → 此前同意，静默上报
 *   - { shouldReport: true,  hasConsent: false } → 无此前同意，弹窗询问
 *   - { shouldReport: false, hasConsent: false } → 已处理，什么都不做
 */
export function checkReportNeeded(version: string): {
  shouldReport: boolean;
  hasConsent: boolean;
} {
  if (isVersionReported(version)) {
    return { shouldReport: false, hasConsent: false };
  }
  // 版本尚未上报。若用户此前同意（hasConsent），静默重试——覆盖用户点「同意」后
  // 发送失败（网络/CSP）的情况：不重弹，直接再试。仅无此前同意时才弹窗。
  const hasConsent = hasStatsConsent();
  return { shouldReport: true, hasConsent };
}
