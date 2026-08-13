import { useCallback, useEffect, useRef, useState } from "react";
import { updater } from "../api";
import type { UpdateInfo } from "../types";

/**
 * 后台更新检查 hook（参考 myshell）。
 *
 * - `enabled`：门控（zcode 无 vault，应用启动后直接传 true）。
 * - `enabled` 翻 true 后**每会话只跑一次**自动检查；`checkNow` 供「关于」页
 *   「检查更新」按钮手动触发。
 * - 永不抛错：失败时 `info` 保留原值（或 null），失败编码进 `info.error`，
 *   UI 把 null / errored 的 info 视为「无更新，保持沉默」。
 */
export function useUpdateCheck(enabled: boolean): {
  info: UpdateInfo | null;
  loading: boolean;
  /** 立即强制重新检查 */
  checkNow: () => void;
} {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [loading, setLoading] = useState(false);
  // 防止 StrictMode + 重渲染下重复触发，并保证自动检查每 enabled-会话至多一次。
  const inFlightRef = useRef(false);
  const autoRanRef = useRef(false);

  const runCheck = useCallback(async () => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    setLoading(true);
    try {
      const result = await updater.checkForUpdates();
      setInfo(result);
    } catch {
      // 防御性：checkForUpdates 失败时 resolve 而非 reject，正常不进这里；
      // 即便进了，保留原 info，不向用户暴露错误。
      setInfo((prev) => prev);
    } finally {
      inFlightRef.current = false;
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    if (autoRanRef.current) return;
    autoRanRef.current = true;
    void runCheck();
  }, [enabled, runCheck]);

  const checkNow = useCallback(() => {
    void runCheck();
  }, [runCheck]);

  return { info, loading, checkNow };
}
