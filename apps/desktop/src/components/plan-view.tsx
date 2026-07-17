import type { ReactNode } from 'react';
import type { OperationPlan } from '../lib/types';

function PlanSection({
  title,
  count,
  tone,
  children,
}: {
  title: string;
  count?: number;
  tone?: 'safe' | 'warn' | 'danger';
  children: ReactNode;
}) {
  return (
    <section className={`planSection ${tone ? `planSection--${tone}` : ''}`}>
      <div className="planSectionHead">
        <h4>{title}</h4>
        {count !== undefined ? <span className="planCount">{count}</span> : null}
      </div>
      {children}
    </section>
  );
}

export function PlanView({ plan }: { plan: OperationPlan | null }) {
  if (!plan) {
    return (
      <div className="planEmpty">
        <p>
          Run <strong>Dry Run</strong> to preview operations before writing any files.
        </p>
      </div>
    );
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
