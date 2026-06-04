import type { ButtonHTMLAttributes, ReactNode } from "react";

type UIButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "ghost" | "icon" | "danger";
  size?: "sm" | "md";
  children: ReactNode;
};

export function UIButton({
  variant = "default",
  size = "md",
  className = "",
  children,
  ...props
}: UIButtonProps) {
  const merged = `uiBtn uiBtn--${variant} uiBtn--${size} ${className}`.trim();
  return (
    <button {...props} className={merged}>
      {children}
    </button>
  );
}
