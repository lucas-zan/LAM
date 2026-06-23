import type { ReactNode } from "react";

type PlanTypeBadgeProps = {
  planType?: string | null;
  className?: string;
};

export function PlanTypeBadge({ planType, className = "" }: PlanTypeBadgeProps) {
  if (!planType) return null;
  const type = planType.trim().toLowerCase();

  let icon: ReactNode = null;
  let customClass = "planTypeBadge";

  if (type === "free") {
    customClass += " planTypeBadge--free";
    icon = (
      <svg
        className="badgeIcon"
        viewBox="0 0 24 24"
        width="10"
        height="10"
        stroke="currentColor"
        strokeWidth="2.5"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="10" strokeDasharray="3 3" />
      </svg>
    );
  } else if (type === "plus") {
    customClass += " planTypeBadge--plus";
    icon = (
      <svg
        className="badgeIcon"
        viewBox="0 0 24 24"
        width="10"
        height="10"
        stroke="currentColor"
        strokeWidth="2.5"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <line x1="12" y1="5" x2="12" y2="19" />
        <line x1="5" y1="12" x2="19" y2="12" />
      </svg>
    );
  } else if (type === "pro") {
    customClass += " planTypeBadge--pro";
    icon = (
      <svg
        className="badgeIcon"
        viewBox="0 0 24 24"
        width="10"
        height="10"
        stroke="currentColor"
        strokeWidth="2.5"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
      </svg>
    );
  } else if (type === "team") {
    customClass += " planTypeBadge--team";
    icon = (
      <svg
        className="badgeIcon"
        viewBox="0 0 24 24"
        width="10"
        height="10"
        stroke="currentColor"
        strokeWidth="2"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
        <path d="M16 3.13a4 4 0 0 1 0 7.75" />
      </svg>
    );
  } else if (type === "business") {
    customClass += " planTypeBadge--business";
    icon = (
      <svg
        className="badgeIcon"
        viewBox="0 0 24 24"
        width="10"
        height="10"
        stroke="currentColor"
        strokeWidth="2.5"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <rect x="2" y="7" width="20" height="14" rx="2" ry="2" />
        <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16" />
      </svg>
    );
  } else {
    customClass += " planTypeBadge--default";
  }

  const merged = `${customClass} ${className}`.trim();

  return (
    <span className={merged}>
      {icon}
      <span>{type.toUpperCase()}</span>
    </span>
  );
}
