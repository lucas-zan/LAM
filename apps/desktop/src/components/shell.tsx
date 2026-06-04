import type { ReactNode } from "react";
import { NavIcon } from "./icons";
import type { Route } from "../routes/types";
import { routes } from "../routes/types";
import { UIButton } from "./ui-button";

export function BottomNav(props: {
  route: Route;
  setRoute: (route: Route) => void;
}) {
  return (
    <nav className="bottomNav" aria-label="Primary">
      {routes.map((item) => (
        <button
          key={item.id}
          type="button"
          onClick={() => props.setRoute(item.id)}
          className={`bottomNavItem ${item.id === props.route ? "active" : ""}`}
          aria-current={item.id === props.route ? "page" : undefined}
        >
          <span className={`bottomNavIcon bottomNavIcon--${item.id}`} aria-hidden>
            <NavIcon name={item.icon} size={16} />
          </span>
          <span className="bottomNavLabel">{item.label}</span>
        </button>
      ))}
    </nav>
  );
}

export function Modal({
  title,
  close,
  children,
  wide = false,
  footer,
}: {
  title: string;
  close: () => void;
  children: ReactNode;
  wide?: boolean;
  footer?: ReactNode;
}) {
  return (
    <div className="overlay">
      <section className={`modal ${wide ? "modalWide" : ""}`}>
        <div className="modalHead">
          <h2>{title}</h2>
          <UIButton type="button" variant="ghost" onClick={close}>
            Close
          </UIButton>
        </div>
        <div className="modalBody">{children}</div>
        {footer ? <div className="modalFoot">{footer}</div> : null}
      </section>
    </div>
  );
}
