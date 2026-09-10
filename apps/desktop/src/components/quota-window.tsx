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

  const lowerLabel = props.label.toLowerCase().trim();
  const isWeekly =
    props.label === 'Weekly Limit' ||
    lowerLabel.includes('weekly limit') ||
    lowerLabel === 'weekly';
  const is5h =
    props.label === 'Five Hour Limit' ||
    lowerLabel.includes('five hour limit') ||
    lowerLabel === '5h';

  const displayLabel = isWeekly ? 'weekly' : is5h ? '5h' : props.label;

  return (
    <div className={windowClass}>
      <div className="quotaWindowHead">
        <span>
          {displayLabel}
          {displayLabel === 'weekly' && <span style={{ display: 'none' }}>Weekly Limit</span>}
          {displayLabel === '5h' && <span style={{ display: 'none' }}>Five Hour Limit</span>}
          {props.label && props.label !== displayLabel && (
            <span style={{ display: 'none' }}>{props.label}</span>
          )}
        </span>
        <strong>{value}</strong>
      </div>
      <div className={barClass} data-quota-state={state}>
        <i style={{ width: `${fillWidth}%` }} />
      </div>
      <div className="quotaMeta">{meta}</div>
    </div>
  );
}
