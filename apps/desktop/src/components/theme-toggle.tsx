import type { ThemeMode } from "../lib/theme";

export function ThemeToggle({ value, onChange }: { value: ThemeMode; onChange: (mode: ThemeMode) => void }) {
  const options: ThemeMode[] = ["system", "light", "dark"];
  return (
    <div className="themeToggle" role="group" aria-label="Theme">
      {options.map((mode) => (
        <button
          key={mode}
          type="button"
          className={value === mode ? "active" : ""}
          onClick={() => onChange(mode)}
        >
          {mode}
        </button>
      ))}
    </div>
  );
}
