import { formatQuotaRemainingLabel, quotaColorState, quotaRemainingPercent } from '../lib/quota';
import type { QuotaWindowVariant } from '../lib/quota';
import { formatResetCountdown } from '../lib/reset';

export function QuotaWindow(props: {
  label: string;
  usedPercent?: number | null;
  resetAt?: string | null;
  variant: QuotaWindowVariant;
  appearance?: 'default' | 'tray';
}) {
  const hasData = props.usedPercent !== null && props.usedPercent !== undefined;
  const remaining = quotaRemainingPercent(props.usedPercent);
  const value = formatQuotaRemainingLabel(props.usedPercent);
  const meta = hasData ? formatResetCountdown(props.resetAt, props.variant) : 'No quota data';
  const state = quotaColorState(props.usedPercent);
  const barClass = `quotaBar quotaBar--${props.variant} quotaBar--${state}`;
  const fillWidth = hasData && state !== 'empty' ? Math.max(4, remaining ?? 0) : 0;

  const windowClass = props.appearance === 'tray' ? 'quotaWindow quotaWindow--tray' : 'quotaWindow';

  return (
    <div className={windowClass}>
      <div className="quotaWindowHead">
        <span>{props.label}</span>
        <strong>{value}</strong>
      </div>
      <div className={barClass} data-quota-state={state}>
        <i style={{ width: `${fillWidth}%` }} />
      </div>
      <div className="quotaMeta">{meta}</div>
    </div>
  );
}
