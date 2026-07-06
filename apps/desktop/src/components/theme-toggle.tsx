import type { ThemeMode } from "../lib/theme";
import { IconSun, IconMoon, IconDevice } from "./icons";

export function ThemeToggle({ value, onChange }: { value: ThemeMode; onChange: (mode: ThemeMode) => void }) {
  const options: ThemeMode[] = ["system", "light", "dark"];

  const getIcon = (mode: ThemeMode) => {
    switch (mode) {
      case "system":
        return <IconDevice size={14} />;
      case "light":
        return <IconSun size={14} />;
      case "dark":
        return <IconMoon size={14} />;
    }
  };

  return (
    <div className="themeToggle" role="group" aria-label="Theme">
      {options.map((mode) => (
        <button
          key={mode}
          type="button"
          className={value === mode ? "active" : ""}
          onClick={() => onChange(mode)}
          title={`Switch to ${mode} theme`}
        >
          {getIcon(mode)}
        </button>
      ))}
    </div>
  );
}
