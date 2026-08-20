interface ProgressProps {
  /** 0~100：进度条宽度 */
  value: number;
  /** 决定颜色档位的值（0~100，默认 = value）。
   *  展示方案为「剩余」时宽度用剩余占比、颜色仍按已用度分级，两者即可分开传 */
  colorValue?: number;
  /** 自定义告警阈值：>60% warn，>85% danger */
  warnAt?: number;
  dangerAt?: number;
  height?: number;
}

export function Progress({
  value,
  colorValue,
  warnAt = 70,
  dangerAt = 90,
  height,
}: ProgressProps) {
  const v = Math.max(0, Math.min(100, value));
  const c = Math.max(0, Math.min(100, colorValue ?? value));
  const level = c >= dangerAt ? "danger" : c >= warnAt ? "warn" : undefined;
  return (
    <div className="za-progress" style={height ? { height } : undefined}>
      <div
        className="za-progress-fill"
        data-level={level}
        style={{ width: `${v}%` }}
      />
    </div>
  );
}
