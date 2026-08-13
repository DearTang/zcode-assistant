interface ProgressProps {
  /** 0~100 */
  value: number;
  /** 自定义告警阈值：>60% warn，>85% danger */
  warnAt?: number;
  dangerAt?: number;
  height?: number;
}

export function Progress({
  value,
  warnAt = 70,
  dangerAt = 90,
  height,
}: ProgressProps) {
  const v = Math.max(0, Math.min(100, value));
  const level =
    v >= dangerAt ? "danger" : v >= warnAt ? "warn" : undefined;
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
