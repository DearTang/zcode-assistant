interface SwitchProps {
  on: boolean;
  onChange: (on: boolean) => void;
  disabled?: boolean;
  title?: string;
}

export function Switch({ on, onChange, disabled, title }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      title={title}
      disabled={disabled}
      className={`za-switch${on ? " is-on" : ""}`}
      onClick={() => !disabled && onChange(!on)}
      style={disabled ? { opacity: 0.4, cursor: "not-allowed" } : undefined}
    >
      <span className="za-switch-thumb" />
    </button>
  );
}
