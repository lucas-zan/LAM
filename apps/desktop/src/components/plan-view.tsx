import type { ReactNode } from "react";
import type { OperationPlan, SyncPlan } from "../lib/types";

function basename(path: string | null | undefined) {
  if (!path) return "—";
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

function SyncPlanGrouped({ plan }: { plan: SyncPlan }) {
  const backup = plan.operations.filter((op) => op.kind === "backup_dir");
  const copy = plan.operations.filter((op) => op.kind === "copy_file");
  const skip = plan.operations.filter((op) => op.kind === "skip_file");
  const sidecar = plan.operations.filter((op) => op.kind === "copy_history_sidecar");
  const maxRows = 12;

  return (
    <div className="planGrouped">
      <div className="planSummary">
        <span className="planStat">
          <em>Will copy</em>
          <strong>{copy.length}</strong>
        </span>
        <span className="planStat">
          <em>Will skip</em>
          <strong>{skip.length}</strong>
        </span>
        <span className="planStat">
          <em>Blocked</em>
          <strong>{plan.blockedFiles.length + plan.policyBlockedFiles.length}</strong>
        </span>
      </div>

      {plan.warnings.length ? (
        <PlanSection title="Warnings" tone="warn">
          <ul className="planList">
            {plan.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </PlanSection>
      ) : null}

      {backup.length ? (
        <PlanSection title="Will backup" tone="safe">
          <ul className="planList mono">
            {backup.map((op, i) => (
              <li key={i}>{basename(op.from?.toString())} → {basename(op.to?.toString())}</li>
            ))}
          </ul>
        </PlanSection>
      ) : null}

      {copy.length ? (
        <PlanSection title="Will copy" count={copy.length}>
          <ul className="planList mono">
            {copy.slice(0, maxRows).map((op, i) => (
              <li key={i}>{op.rel ?? basename(op.from?.toString())}</li>
            ))}
            {copy.length > maxRows ? <li className="planMore">…and {copy.length - maxRows} more session files</li> : null}
          </ul>
        </PlanSection>
      ) : null}

      {skip.length ? (
        <PlanSection title="Will skip" count={skip.length}>
          <ul className="planList mono faint">
            {skip.slice(0, maxRows).map((op, i) => (
              <li key={i}>{op.rel ?? basename(op.from?.toString())}</li>
            ))}
            {skip.length > maxRows ? <li className="planMore">…and {skip.length - maxRows} more</li> : null}
          </ul>
        </PlanSection>
      ) : null}

      {sidecar.length ? (
        <PlanSection title="History sidecar">
          <ul className="planList mono">
            {sidecar.map((op, i) => (
              <li key={i}>{basename(op.to?.toString())}</li>
            ))}
          </ul>
        </PlanSection>
      ) : null}

      {plan.policyBlockedFiles.length ? (
        <PlanSection title="Never copied (policy)" tone="danger">
          <ul className="planList mono">
            {plan.policyBlockedFiles.map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        </PlanSection>
      ) : null}

      {plan.blockedFiles.length ? (
        <PlanSection title="Blocked in source tree" tone="danger">
          <ul className="planList mono">
            {plan.blockedFiles.slice(0, maxRows).map((f) => (
              <li key={f}>{f}</li>
            ))}
            {plan.blockedFiles.length > maxRows ? (
              <li className="planMore">…and {plan.blockedFiles.length - maxRows} more paths</li>
            ) : null}
          </ul>
        </PlanSection>
      ) : null}
    </div>
  );
}

function PlanSection({
  title,
  count,
  tone,
  children,
}: {
  title: string;
  count?: number;
  tone?: "safe" | "warn" | "danger";
  children: ReactNode;
}) {
  return (
    <section className={`planSection ${tone ? `planSection--${tone}` : ""}`}>
      <div className="planSectionHead">
        <h4>{title}</h4>
        {count !== undefined ? <span className="planCount">{count}</span> : null}
      </div>
      {children}
    </section>
  );
}

export function PlanView({ plan }: { plan: OperationPlan | SyncPlan | null }) {
  if (!plan) {
    return (
      <div className="planEmpty">
        <p>Run <strong>Dry Run</strong> to preview operations before writing any files.</p>
      </div>
    );
  }

  if ("blockedFiles" in plan) {
    return <SyncPlanGrouped plan={plan} />;
  }

  return (
    <div className="planGrouped">
      {plan.warnings.length ? (
        <PlanSection title="Warnings" tone="warn">
          <ul className="planList">
            {plan.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </PlanSection>
      ) : null}
      <PlanSection title="Operations">
        <ul className="planList mono">
          {plan.operations.map((op) => (
            <li key={op}>{op}</li>
          ))}
        </ul>
      </PlanSection>
      {plan.blocked.length ? (
        <PlanSection title="Blocked" tone="danger">
          <ul className="planList mono">
            {plan.blocked.map((b) => (
              <li key={b}>{b}</li>
            ))}
          </ul>
        </PlanSection>
      ) : null}
    </div>
  );
}
