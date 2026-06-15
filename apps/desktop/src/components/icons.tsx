import type { ImgHTMLAttributes, ReactNode, SVGProps } from "react";
import lamOrbitIcon from "../assets/lam-orbit-icon.svg";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Svg({ size = 18, children, ...props }: IconProps & { children: ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden {...props}>
      {children}
    </svg>
  );
}

export function IconLogo({
  size = 24,
  bare = false,
  className = "",
  ...props
}: ImgHTMLAttributes<HTMLImageElement> & { size?: number; bare?: boolean }) {
  void bare;
  return (
    <img
      src={lamOrbitIcon}
      width={size}
      height={size}
      alt=""
      aria-hidden
      className={`LAMLogo LAMOrbitLogo ${className}`.trim()}
      {...props}
    />
  );
}

export function IconOverview(props: IconProps) {
  return <Svg {...props}><path d="M3 10.5 12 4l9 6.5V20a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" /><path d="M9 21V12h6v9" /></Svg>;
}
export function IconSessions(props: IconProps) {
  return <Svg {...props}><path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" /></Svg>;
}
export function IconRelay(props: IconProps) {
  return <Svg {...props}><path d="M4 12h4l2-5 4 10 2-5h4" /></Svg>;
}
export function IconProviders(props: IconProps) {
  return <Svg {...props}><path d="M12 2 2 7l10 5 10-5-10-5z" /><path d="M2 17l10 5 10-5M2 12l10 5 10-5" /></Svg>;
}
export function IconSync(props: IconProps) {
  return <Svg {...props}><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path d="M16 16h5v5" /></Svg>;
}
export function IconSettings(props: IconProps) {
  return <Svg {...props}><circle cx="12" cy="12" r="3" /><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" /></Svg>;
}
export function IconRefresh(props: IconProps) {
  return <Svg {...props}><path d="M21 12a9 9 0 1 1-2.64-6.36" /><path d="M21 3v6h-6" /></Svg>;
}
export function IconUsers(props: IconProps) {
  return <Svg {...props}><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75" /></Svg>;
}
export function IconActivity(props: IconProps) {
  return <Svg {...props}><path d="M22 12h-4l-3 9L9 3l-3 9H2" /></Svg>;
}
export function IconClock(props: IconProps) {
  return <Svg {...props}><circle cx="12" cy="12" r="10" /><path d="M12 6v6l4 2" /></Svg>;
}
export function IconCopy(props: IconProps) {
  return <Svg {...props}><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></Svg>;
}
export function IconInfo(props: IconProps) {
  return <Svg {...props}><circle cx="12" cy="12" r="10" /><path d="M12 16v-4M12 8h.01" /></Svg>;
}
export function IconExternalLink(props: IconProps) {
  return <Svg {...props}><path d="M15 3h6v6" /><path d="M10 14 21 3" /><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5" /></Svg>;
}
export function IconClose(props: IconProps) {
  return <Svg {...props} strokeWidth={2}><path d="M18 6 6 18M6 6l12 12" /></Svg>;
}
export function IconSun(props: IconProps) {
  return (
    <Svg {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
    </Svg>
  );
}
export function IconMoon(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />
    </Svg>
  );
}

export type NavIconName = "overview" | "sessions" | "relay" | "providers" | "sync" | "settings";

const navIcons: Record<NavIconName, (p: IconProps) => JSX.Element> = {
  overview: IconOverview,
  sessions: IconSessions,
  relay: IconRelay,
  providers: IconProviders,
  sync: IconSync,
  settings: IconSettings,
};

export function NavIcon({ name, ...props }: { name: NavIconName } & IconProps) {
  const C = navIcons[name];
  return <C {...props} />;
}

export type MetricIconName = "accounts" | "sessions" | "providers" | "quota";

export function MetricIcon({ name, ...props }: { name: MetricIconName } & IconProps) {
  if (name === "accounts") return <IconUsers {...props} />;
  if (name === "sessions") return <IconActivity {...props} />;
  if (name === "providers") return <IconProviders {...props} />;
  return <IconClock {...props} />;
}
