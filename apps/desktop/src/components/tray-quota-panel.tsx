import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import {
  hideQuotaPopover,
  listAccounts,
  listCachedAccounts,
  listCachedQuotas,
  listSessions,
  openTerminalWithCommand,
  openTerminalWithResume,
  relayResumeSession,
  getProfileQuota,
  inTauri,
  setQuotaPopoverOpacity,
  getAntigravityQuota,
} from '../lib/api';
import {
  averagePrimaryRemainingPercent,
  countAccountsWithAvailableQuota,
  countAccountsWithQuotaData,
  mergeQuotaSnapshots,
  quotaRemainingPercent,
} from '../lib/quota';
import { formatResetCountdown } from '../lib/reset';
import { scheduleTrayPopoverWindowSize } from '../lib/tray-popover-size';
import type { ThemeMode } from '../lib/theme';
import { TRAY_POPOVER_OPACITY_PERCENT } from '../lib/tray-popover-prefs';
import type {
  CodexAccount,
  CodexSession,
  DivergedSessionStrategy,
  UsageQuotaSnapshot,
  AntigravityQuotaResponse,
} from '../lib/types';
import {
  IconClock,
  IconClose,
  IconCopy,
  IconExternalLink,
  IconLogo,
  IconRefresh,
  IconSun,
  IconMoon,
} from './icons';
import { UIButton } from './ui-button';

type ProviderGroup = {
  id: string;
  title: string;
  meta: string;
  accounts: CodexAccount[];
};

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

function resolveThemeMode(): ThemeMode {
  const saved = localStorage.getItem('lam-theme');
  return saved === 'light' || saved === 'dark' || saved === 'system' ? saved : 'system';
}

function readDivergedStrategy(): DivergedSessionStrategy {
  const saved = localStorage.getItem('lam-diverged-session-strategy');
  if (
    saved === 'stop_and_ask' ||
    saved === 'summarize_fork_with_target_account' ||
    saved === 'timeline_merge_to_fork' ||
    saved === 'prefer_source' ||
    saved === 'prefer_target'
  ) {
    return saved;
  }
  return 'summarize_fork_with_target_account';
}

function sortByLatestActivity(accounts: CodexAccount[]) {
  return [...accounts].sort((a, b) => {
    const aLatest = a.latestSessionModifiedAt ?? 0;
    const bLatest = b.latestSessionModifiedAt ?? 0;
    const latestDiff = bLatest - aLatest;
    if (latestDiff !== 0) return latestDiff;
    const sessionDiff = b.sessionCount - a.sessionCount;
    if (sessionDiff !== 0) return sessionDiff;
    return a.displayName.localeCompare(b.displayName);
  });
}

function readResolvedTheme(): 'light' | 'dark' {
  const mode = resolveThemeMode();
  if (mode === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return mode;
}

const ACCOUNT_THEMES = [
  {
    // Orange/Amber
    light: { color: '#d97706', glow: 'rgba(217, 119, 6, 0.3)' },
    dark: { color: '#fbbf24', glow: 'rgba(251, 191, 36, 0.4)' },
  },
  {
    // Blue
    light: { color: '#0284c7', glow: 'rgba(2, 132, 199, 0.3)' },
    dark: { color: '#38bdf8', glow: 'rgba(56, 189, 248, 0.4)' },
  },
  {
    // Pink/Rose
    light: { color: '#db2777', glow: 'rgba(219, 39, 119, 0.3)' },
    dark: { color: '#f472b6', glow: 'rgba(244, 114, 182, 0.4)' },
  },
  {
    // Purple/Violet
    light: { color: '#7c3aed', glow: 'rgba(124, 58, 237, 0.3)' },
    dark: { color: '#a78bfa', glow: 'rgba(167, 139, 250, 0.4)' },
  },
  {
    // Green/Emerald
    light: { color: '#059669', glow: 'rgba(5, 150, 105, 0.3)' },
    dark: { color: '#34d399', glow: 'rgba(52, 211, 153, 0.4)' },
  },
];

function getAccountTheme(accountName: string, index: number, isDark: boolean) {
  let themeIndex = index;
  if (accountName) {
    let hash = 0;
    for (let i = 0; i < accountName.length; i++) {
      hash = accountName.charCodeAt(i) + ((hash << 5) - hash);
    }
    themeIndex = Math.abs(hash);
  }
  const theme = ACCOUNT_THEMES[themeIndex % ACCOUNT_THEMES.length];
  return isDark ? theme.dark : theme.light;
}

const STATE_THEMES = {
  safe: {
    light: { color: '#16a34a', glow: 'rgba(22, 163, 74, 0.3)' },
    dark: { color: '#34d399', glow: 'rgba(52, 211, 153, 0.4)' },
  },
  warn: {
    light: { color: '#d97706', glow: 'rgba(217, 119, 6, 0.3)' },
    dark: { color: '#fbbf24', glow: 'rgba(251, 191, 36, 0.4)' },
  },
  danger: {
    light: { color: '#dc2626', glow: 'rgba(220, 38, 38, 0.3)' },
    dark: { color: '#f87171', glow: 'rgba(248, 113, 113, 0.4)' },
  },
  empty: {
    light: { color: '#dc2626', glow: 'rgba(220, 38, 38, 0.3)' },
    dark: { color: '#f87171', glow: 'rgba(248, 113, 113, 0.4)' },
  },
  na: {
    light: { color: '#94a3b8', glow: 'rgba(148, 163, 184, 0.2)' },
    dark: { color: '#64748b', glow: 'rgba(100, 116, 139, 0.2)' },
  },
};

function getQuotaStateTheme(percent: number | null, isDark: boolean) {
  let state: keyof typeof STATE_THEMES = 'na';
  if (percent !== null) {
    if (percent === 0) {
      state = 'empty';
    } else if (percent < 25) {
      state = 'danger';
    } else if (percent < 70) {
      state = 'warn';
    } else {
      state = 'safe';
    }
  }
  const theme = STATE_THEMES[state];
  return isDark ? theme.dark : theme.light;
}

interface CircularProgressRingProps {
  percent: number | null;
  size?: number;
  strokeWidth?: number;
  themeColor: string;
  themeGlow: string;
}

function CircularProgressRing({
  percent,
  size = 50,
  strokeWidth = 4.5,
  themeColor,
  themeGlow,
}: CircularProgressRingProps) {
  const val = percent === null ? 0 : percent;
  const radius = (size - strokeWidth) / 2;
  const circumference = radius * 2 * Math.PI;
  const strokeDashoffset = circumference - (val / 100) * circumference;

  return (
    <div className="circularProgressWrapper" style={{ width: size, height: size }}>
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className="circularProgressSvg"
      >
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          className="circularProgressTrack"
          strokeWidth={strokeWidth}
        />
        {percent !== null && (
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            className="circularProgressIndicator"
            strokeWidth={strokeWidth}
            strokeDasharray={circumference}
            strokeDashoffset={strokeDashoffset}
            stroke={themeColor}
            style={{
              filter: `drop-shadow(0 0 2.5px ${themeGlow})`,
            }}
          />
        )}
      </svg>
      <div className="circularProgressText" style={{ color: themeColor }}>
        <strong>{percent === null ? 'N/A' : `${percent}%`}</strong>
      </div>
    </div>
  );
}

interface TrayPopoverHeaderProps {
  status: string;
  refreshing: boolean;
  onRefresh: () => void;
  onClose: () => void;
  accountsWithQuotaData: number;
  totalAccounts: number;
  avg5hRemaining: number | null;
  availableQuotaAccounts: number;
  activeAccountDisplayName?: string;
  activeSessionId?: string;
  resolvedTheme: 'light' | 'dark';
  onToggleTheme: () => void;
  activeProviderId?: string;
}

function TrayPopoverHeader({
  status,
  refreshing,
  onRefresh,
  onClose,
  accountsWithQuotaData,
  totalAccounts,
  avg5hRemaining,
  availableQuotaAccounts,
  activeAccountDisplayName,
  activeSessionId,
  resolvedTheme,
  onToggleTheme,
  activeProviderId,
}: TrayPopoverHeaderProps) {
  const handleCopy = () => {
    if (activeSessionId) {
      void navigator.clipboard.writeText(activeSessionId);
    }
  };

  const isAntigravity = activeProviderId === 'antigravity';

  return (
    <section className="trayPopoverFixedHead" aria-label="Quota header">
      <header className="trayPopoverHead">
        <div className="trayBrand">
          <span className="trayBrandMark" aria-hidden>
            <IconLogo size={24} />
          </span>
          <div>
            <h2>LAM Quota</h2>
            <p>
              <IconClock size={12} /> {status}
            </p>
          </div>
        </div>
        <div className="trayPopoverHeadActions">
          <button
            type="button"
            className="trayPopoverIconBtn trayThemeToggleButton"
            aria-label={
              resolvedTheme === 'light' ? 'Switch to dark theme' : 'Switch to light theme'
            }
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onToggleTheme();
            }}
          >
            {resolvedTheme === 'light' ? <IconMoon size={14} /> : <IconSun size={14} />}
          </button>
          <button
            type="button"
            className={`trayPopoverIconBtn trayRefreshButton ${refreshing ? 'isRefreshing' : ''}`}
            aria-label={refreshing ? 'Refreshing quotas' : 'Refresh quotas'}
            aria-busy={refreshing}
            disabled={refreshing}
            onClick={onRefresh}
          >
            <IconRefresh size={14} />
          </button>
          <button
            type="button"
            className="trayPopoverIconBtn trayDismissButton"
            aria-label="Close panel"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onClose();
            }}
          >
            <IconClose size={14} />
          </button>
        </div>
      </header>

      <section className="trayStats" aria-label="Quota summary">
        <div>
          <span>{isAntigravity ? 'Models' : 'Accounts'}</span>
          <strong>
            {accountsWithQuotaData}/{totalAccounts}
          </strong>
        </div>
        <div>
          <span>{isAntigravity ? 'Avg Quota' : '5h avg'}</span>
          <strong>{avg5hRemaining === null ? 'N/A' : `${avg5hRemaining}%`}</strong>
        </div>
        <div>
          <span>Usable</span>
          <strong>{availableQuotaAccounts}</strong>
        </div>
      </section>

      <section className="trayActiveSource" aria-label="Active source session">
        <div className="trayActiveSourceBar">
          <span className="activeDot" />
          <span className="activeLabel">Active</span>
          <strong className="activeAccount" title={activeAccountDisplayName}>
            {activeAccountDisplayName ?? 'No session'}
          </strong>
          <span className="activeSessionId">{activeSessionId ?? 'No active session'}</span>
          {activeSessionId && (
            <button
              type="button"
              className="trayActiveSourceCopyBtn"
              title="Copy session ID"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                handleCopy();
              }}
            >
              <IconCopy size={12} />
            </button>
          )}
        </div>
      </section>
    </section>
  );
}

interface TrayPopoverFooterProps {
  onClose: () => void;
  onOpen: () => void;
}

function TrayPopoverFooter({ onClose, onOpen }: TrayPopoverFooterProps) {
  return (
    <footer className="trayPopoverFoot">
      <div className="trayPopoverActions">
        <UIButton
          size="sm"
          variant="ghost"
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          }}
        >
          <IconClose size={13} />
          Close
        </UIButton>
        <span />
        <UIButton
          size="sm"
          variant="primary"
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onOpen();
          }}
        >
          <IconExternalLink size={13} />
          Open
        </UIButton>
      </div>
    </footer>
  );
}

function formatRelativeTime(resetTimeStr: string | null | undefined): string {
  if (!resetTimeStr) return 'unknown';
  const date = new Date(resetTimeStr);
  if (Number.isNaN(date.getTime())) return 'unknown';
  const diffMs = date.getTime() - Date.now();
  if (diffMs <= 0) return 'now';

  const diffSecs = Math.floor(diffMs / 1000);
  const hours = Math.floor(diffSecs / 3600);
  const minutes = Math.floor((diffSecs % 3600) / 60);
  const seconds = diffSecs % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }
  return `${seconds}s`;
}

interface TrayAntigravityModelListProps {
  quota: AntigravityQuotaResponse | null;
  isDark: boolean;
}

function TrayAntigravityModelList({
  quota,
  isDark,
}: TrayAntigravityModelListProps) {
  const [_, setTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => {
      setTick((t) => t + 1);
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  if (!quota) {
    return (
      <div className="trayProviderRows">
        <p className="trayPopoverEmpty">Loading Antigravity status...</p>
      </div>
    );
  }

  if (!quota.ok) {
    return (
      <div className="trayProviderRows">
        <div className="trayPopoverEmpty" style={{ padding: '24px 16px', textAlign: 'center' }}>
          <div style={{ fontSize: '24px', marginBottom: '8px' }}>🔌</div>
          <p style={{ margin: 0, fontSize: '13px', fontWeight: 500, color: 'var(--text-muted)' }}>
            Antigravity Offline
          </p>
          <p style={{ margin: '4px 0 0 0', fontSize: '11px', color: 'var(--text-muted)', opacity: 0.8 }}>
            {quota.error || 'Server is not running'}
          </p>
        </div>
      </div>
    );
  }

  if (quota.models.length === 0) {
    return (
      <div className="trayProviderRows">
        <p className="trayPopoverEmpty">No Antigravity models found.</p>
      </div>
    );
  }

  return (
    <div className="trayProviderRows">
      {quota.models.map((model, index) => {
        const remainingFraction = model.remainingFraction ?? null;
        const remainingPercent = remainingFraction !== null ? Math.round(remainingFraction * 100) : null;
        const isDepleted = remainingPercent === 0;

        const modelTheme = getAccountTheme(model.label, index, isDark);
        const stateTheme = getQuotaStateTheme(remainingPercent, isDark);

        const cardStyle = {
          borderColor: modelTheme.color + '22',
        } as CSSProperties;

        return (
          <div className="trayAccountRow" key={model.label} style={cardStyle}>
            <div className="trayAccountRowTop">
              <div className="trayAccountMain">
                <span
                  className="trayAccountStatusDot"
                  style={{
                    backgroundColor: isDepleted ? '#ef4444' : '#22c55e',
                    boxShadow: isDepleted ? '0 0 4px rgba(239, 68, 68, 0.5)' : '0 0 4px rgba(34, 197, 94, 0.5)',
                  }}
                />
                <div className="trayAccountNameWrap" style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <strong title={model.label}>{model.label}</strong>
                  {isDepleted && (
                    <span style={{ fontSize: '12px' }} title="Quota depleted" role="img" aria-label="warning">⚠️</span>
                  )}
                </div>
              </div>
            </div>

            <div className="trayAccountRowContent">
              <div className="trayAccountRowContentLeft">
                <div className="trayAccountRowContentLeftText">
                  <strong>Quota</strong>
                  <span>remaining</span>
                </div>
                <CircularProgressRing
                  percent={remainingPercent}
                  themeColor={stateTheme.color}
                  themeGlow={stateTheme.glow}
                />
              </div>

              <div className="trayAccountRowContentRight">
                <div className="trayAccountRowContentRightLabel">
                  <strong style={{ color: stateTheme.color }}>
                    {remainingPercent === null ? 'N/A' : `${remainingPercent}%`}
                  </strong>
                  <span>limit</span>
                </div>
                <div className="trayQuotaTrack">
                  <i
                    style={{
                      width: `${remainingPercent ?? 0}%`,
                      background: stateTheme.color,
                      boxShadow: `0 0 4px ${stateTheme.glow}`,
                    }}
                  />
                </div>
                <span className="trayResetSub" style={{ textAlign: 'right' }}>
                  {model.resetTime ? `Resets in ${formatRelativeTime(model.resetTime)}` : 'No reset scheduled'}
                </span>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

interface TrayAccountListProps {
  providerGroups: ProviderGroup[];
  activeProviderId: string;
  setActiveProviderId: (id: string) => void;
  quotas: UsageQuotaSnapshot[];
  activeSession?: CodexSession;
  relayingAccountId: string;
  refreshingQuotaIds: string[];
  onRefreshAccount: (account: CodexAccount) => void;
  onRelayTo: (account: CodexAccount) => void;
  isDark: boolean;
  antigravityQuota: AntigravityQuotaResponse | null;
}

function TrayAccountList({
  providerGroups,
  activeProviderId,
  setActiveProviderId,
  quotas,
  activeSession,
  relayingAccountId,
  refreshingQuotaIds,
  onRefreshAccount,
  onRelayTo,
  isDark,
  antigravityQuota,
}: TrayAccountListProps) {
  const activeGroup =
    providerGroups.find((group) => group.id === activeProviderId) ?? providerGroups[0];
  const showProviderTabs = providerGroups.length > 1;

  if (!activeGroup) {
    return (
      <section className="trayAccountList" aria-label="Accounts">
        <p className="trayPopoverEmpty">No profiles found.</p>
      </section>
    );
  }

  return (
    <section className="trayAccountList" aria-label="Accounts">
      {showProviderTabs ? (
        <div className="trayProviderTabs" role="tablist" aria-label="Provider groups">
          {providerGroups.map((group) => (
            <button
              key={group.id}
              type="button"
              role="tab"
              id={`tray-tab-${group.id}`}
              aria-selected={group.id === activeGroup.id}
              aria-controls={`tray-panel-${group.id}`}
              className={`trayProviderTab ${group.id === activeGroup.id ? 'isActive' : ''}`}
              onClick={() => setActiveProviderId(group.id)}
            >
              {group.title}
              {group.id === 'codex' && <em>{group.accounts.length}</em>}
              {group.id === 'antigravity' && antigravityQuota?.ok && <em>{antigravityQuota.models.length}</em>}
            </button>
          ))}
        </div>
      ) : null}

      <div
        className="trayProviderPanel"
        role="tabpanel"
        id={`tray-panel-${activeGroup.id}`}
        aria-labelledby={showProviderTabs ? `tray-tab-${activeGroup.id}` : undefined}
      >
        {showProviderTabs ? (
          <div className="trayProviderPanelMeta">
            <span>{activeGroup.meta}</span>
          </div>
        ) : null}

        {activeGroup.id === 'antigravity' ? (
          <TrayAntigravityModelList
            quota={antigravityQuota}
            isDark={isDark}
          />
        ) : activeGroup.accounts.length === 0 ? (
          <div className="trayProviderRows">
            <p className="trayPopoverEmpty">No Codex profiles found.</p>
          </div>
        ) : (
          <div className="trayProviderRows">
          {activeGroup.accounts.map((account, index) => {
            const quota = quotas.find((q) => q.profileId === account.id);
            const title = account.displayName.trim() || account.id;
            const isActiveAccount = activeSession?.accountId === account.id;
            const isRefreshingQuota = refreshingQuotaIds.includes(account.id);

            const accountTheme = getAccountTheme(account.id, index, isDark);
            const primaryRemaining = quotaRemainingPercent(quota?.primaryUsedPercent);
            const secondaryRemaining = quotaRemainingPercent(quota?.secondaryUsedPercent);

            const primaryStateTheme = getQuotaStateTheme(primaryRemaining, isDark);
            const secondaryStateTheme = getQuotaStateTheme(secondaryRemaining, isDark);

            const cardStyle = {
              borderColor: accountTheme.color + '22',
            } as CSSProperties;

            const btnStyle = {
              '--btn-bg': accountTheme.color + '14',
              '--btn-text': accountTheme.color,
              '--btn-border': accountTheme.color + '22',
            } as CSSProperties;

            return (
              <div className="trayAccountRow" key={account.id} style={cardStyle}>
                <div className="trayAccountRowTop">
                  <div className="trayAccountMain">
                    <span className={`trayAccountStatusDot ${isActiveAccount ? 'isActive' : ''}`} />
                    <div className="trayAccountNameWrap">
                      <strong title={title}>{title}</strong>
                      {isActiveAccount && (
                        <span className="accountActiveBadge" aria-label="Active session account">
                          Active
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="trayAccountActions">
                    <button
                      type="button"
                      className={`trayAccountRefreshButton ${isRefreshingQuota ? 'isRefreshing' : ''}`}
                      aria-label={`Refresh ${title} quota`}
                      title={`Refresh ${title} quota`}
                      disabled={isRefreshingQuota}
                      onClick={() => onRefreshAccount(account)}
                    >
                      <IconRefresh size={12} />
                    </button>
                    <button
                      type="button"
                      className="trayRelayButton"
                      style={btnStyle}
                      disabled={!activeSession || relayingAccountId === account.id}
                      onClick={() => onRelayTo(account)}
                    >
                      {activeSession?.accountId === account.id ? 'Resume' : 'Relay'}
                    </button>
                  </div>
                </div>

                <div className="trayAccountRowContent">
                  <div className="trayAccountRowContentLeft">
                    <div className="trayAccountRowContentLeftText">
                      <strong>5h</strong>
                      <span>{formatResetCountdown(quota?.resetAt, 'session')}</span>
                    </div>
                    <CircularProgressRing
                      percent={primaryRemaining}
                      themeColor={primaryStateTheme.color}
                      themeGlow={primaryStateTheme.glow}
                    />
                  </div>

                  <div className="trayAccountRowContentRight">
                    <div className="trayAccountRowContentRightLabel">
                      <strong style={{ color: secondaryStateTheme.color }}>
                        {secondaryRemaining === null ? 'N/A' : `${secondaryRemaining}%`}
                      </strong>
                      <span>weekly</span>
                    </div>
                    <div className="trayQuotaTrack">
                      <i
                        style={{
                          width: `${secondaryRemaining ?? 0}%`,
                          background: secondaryStateTheme.color,
                          boxShadow: `0 0 4px ${secondaryStateTheme.glow}`,
                        }}
                      />
                    </div>
                    <span className="trayResetSub" style={{ textAlign: 'right' }}>
                      {formatResetCountdown(quota?.secondaryResetAt, 'weekly')}
                    </span>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
        )}
      </div>
    </section>
  );
}

export function TrayQuotaPanel() {
  const [accounts, setAccounts] = useState<CodexAccount[]>([]);
  const [quotas, setQuotas] = useState<UsageQuotaSnapshot[]>([]);
  const [activeSession, setActiveSession] = useState<CodexSession | undefined>(undefined);
  const [status, setStatus] = useState('Loading…');
  const [refreshing, setRefreshing] = useState(false);
  const [refreshingQuotaIds, setRefreshingQuotaIds] = useState<string[]>([]);
  const [relayingAccountId, setRelayingAccountId] = useState<string>('');
  const [activeProviderId, setActiveProviderId] = useState('codex');
  const [antigravityQuota, setAntigravityQuota] = useState<AntigravityQuotaResponse | null>(null);
  const [refreshingAntigravity, setRefreshingAntigravity] = useState(false);
  const [resolvedTheme, setResolvedTheme] = useState<'light' | 'dark'>(() => readResolvedTheme());
  const panelRef = useRef<HTMLDivElement>(null);

  const applyTheme = useCallback(() => {
    const resolved = readResolvedTheme();
    setResolvedTheme(resolved);
    document.documentElement.dataset.theme = resolved;
  }, []);

  const handleToggleTheme = useCallback(() => {
    const nextTheme = resolvedTheme === 'light' ? 'dark' : 'light';
    localStorage.setItem('lam-theme', nextTheme);
    setResolvedTheme(nextTheme);
    document.documentElement.dataset.theme = nextTheme;
    window.dispatchEvent(
      new StorageEvent('storage', {
        key: 'lam-theme',
        newValue: nextTheme,
      }),
    );
  }, [resolvedTheme]);

  /* eslint-disable react-hooks/set-state-in-effect -- syncing with external media query */
  useEffect(() => {
    document.documentElement.dataset.trayPopover = '1';
    applyTheme();
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => applyTheme();
    media.addEventListener('change', onChange);
    const onStorage = (event: StorageEvent) => {
      if (event.key === 'lam-theme') applyTheme();
    };
    window.addEventListener('storage', onStorage);
    return () => {
      delete document.documentElement.dataset.trayPopover;
      media.removeEventListener('change', onChange);
      window.removeEventListener('storage', onStorage);
    };
  }, [applyTheme]);
  /* eslint-enable react-hooks/set-state-in-effect */

  useEffect(() => {
    if (inTauri()) {
      void setQuotaPopoverOpacity(TRAY_POPOVER_OPACITY_PERCENT);
    }
  }, []);

  const loadActiveSession = useCallback(async (accountData: CodexAccount[]) => {
    const results = await Promise.allSettled(
      accountData.map((account) => listSessions(account.id)),
    );
    const allSessions = results.flatMap((result) =>
      result.status === 'fulfilled' ? result.value : [],
    );
    setActiveSession(allSessions.sort((a, b) => b.modifiedAt - a.modifiedAt)[0]);
  }, []);

  const loadAntigravity = useCallback(async (forceRefresh = false) => {
    if (!inTauri()) return;
    if (forceRefresh) {
      setRefreshingAntigravity(true);
    }
    try {
      const res = await getAntigravityQuota();
      setAntigravityQuota(res);
    } catch (err) {
      console.error('Failed to load Antigravity quota:', err);
    } finally {
      setRefreshingAntigravity(false);
    }
  }, []);

  const load = useCallback(
    async (forceRefresh = false) => {
      if (!inTauri()) {
        setStatus('Tray panel requires the desktop app.');
        return;
      }
      if (forceRefresh) {
        setRefreshing(true);
        setStatus('Refreshing…');
      }
      try {
        if (!forceRefresh) {
          try {
            const cached = await listCachedAccounts();
            if (cached.length) {
              setAccounts(cached);
              void loadActiveSession(cached);
              const cachedIds = cached.map((a) => a.id);
              const cachedQuotas = await listCachedQuotas(cachedIds.length ? cachedIds : undefined);
              setQuotas(cachedQuotas);
              setStatus(
                `Cached · ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
              );
            }
          } catch {
            /* cache miss is fine */
          }
        }

        const accountData = await listAccounts();
        setAccounts(accountData);
        void loadActiveSession(accountData);
        const ids = accountData.map((a) => a.id);
        if (forceRefresh && ids.length) {
          let completed = 0;
          let unavailable = 0;
          await Promise.all(
            ids.map(async (profileId) => {
              try {
                const snapshot = await getProfileQuota(profileId, true);
                setQuotas((current) => mergeQuotaSnapshots(current, snapshot));
                completed += 1;
                if (snapshot.staleness !== 'fresh') unavailable += 1;
              } catch (err) {
                unavailable += 1;
                setStatus(`${profileId}: ${formatError(err)}`);
              }
            }),
          );
          const suffix = unavailable ? ` · ${unavailable} unavailable` : '';
          setStatus(
            `Updated ${completed}/${ids.length}${suffix} · ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
          );
        } else {
          const cached = await listCachedQuotas(ids.length ? ids : undefined);
          setQuotas(cached);
          setStatus(
            `Cached · ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
          );
        }
      } catch (err) {
        setStatus(formatError(err));
      } finally {
        setRefreshing(false);
        scheduleTrayPopoverWindowSize(panelRef.current);
      }
    },
    [loadActiveSession],
  );

  const handleRefreshAll = useCallback(async () => {
    setRefreshing(true);
    setStatus('Refreshing…');
    try {
      await Promise.all([
        load(true),
        loadAntigravity(true),
      ]);
    } finally {
      setRefreshing(false);
    }
  }, [load, loadAntigravity]);

  /* eslint-disable react-hooks/set-state-in-effect -- async data fetch on mount + interval */
  useEffect(() => {
    void load(false);
    void loadAntigravity(false);
    const timer = window.setInterval(() => {
      void load(true);
      void loadAntigravity(true);
    }, 2 * 60_000);
    const unlisten = listen('quota-popover-refresh', () => {
      void load(false);
      void loadAntigravity(false);
    });
    return () => {
      window.clearInterval(timer);
      void unlisten.then((fn) => fn());
    };
  }, [load, loadAntigravity]);
  /* eslint-enable react-hooks/set-state-in-effect */

  async function closePopover() {
    if (inTauri()) {
      await hideQuotaPopover();
      return;
    }
    await getCurrentWebviewWindow().hide();
  }

  async function openMain() {
    await invoke('show_main_window');
    await closePopover();
  }

  async function refreshAccountQuota(account: CodexAccount) {
    setRefreshingQuotaIds((ids) => Array.from(new Set([...ids, account.id])));
    setStatus(`Refreshing ${account.displayName}...`);
    try {
      const snapshot = await getProfileQuota(account.id, true);
      setQuotas((current) => mergeQuotaSnapshots(current, snapshot));
      const suffix = snapshot.staleness === 'fresh' ? '' : ` · ${snapshot.staleness}`;
      setStatus(
        `Updated ${account.displayName}${suffix} · ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
      );
    } catch (err) {
      setStatus(`${account.id}: ${formatError(err)}`);
    } finally {
      setRefreshingQuotaIds((ids) => ids.filter((id) => id !== account.id));
      scheduleTrayPopoverWindowSize(panelRef.current);
    }
  }

  async function relayTo(account: CodexAccount) {
    if (!activeSession) {
      setStatus('No active session found.');
      return;
    }
    setRelayingAccountId(account.id);
    try {
      if (account.id !== activeSession.accountId) {
        const result = await relayResumeSession({
          fromProfileId: activeSession.accountId,
          toProfileId: account.id,
          sessionId: activeSession.id,
          cwd: activeSession.cwd,
          divergedStrategy: readDivergedStrategy(),
        });
        await openTerminalWithCommand(result.resume.command);
      } else {
        await openTerminalWithResume({
          profileId: account.id,
          sessionId: activeSession.id,
          cwd: activeSession.cwd,
        });
      }
      setStatus(`Resume ${activeSession.id} on ${account.id}`);
      await loadActiveSession(accounts);
    } catch (err) {
      setStatus(formatError(err));
    } finally {
      setRelayingAccountId('');
    }
  }

  const isAntigravityActive = activeProviderId === 'antigravity';

  const accountsWithQuotaData = countAccountsWithQuotaData(accounts, quotas);
  const availableQuotaAccounts = countAccountsWithAvailableQuota(accounts, quotas);
  const avg5hRemaining = averagePrimaryRemainingPercent(quotas);

  const antigravityModelsCount = antigravityQuota?.models.length ?? 0;
  const antigravityUsableCount = antigravityQuota?.models.filter(m => (m.remainingFraction ?? 0) > 0).length ?? 0;
  const antigravityAvgRemaining = useMemo(() => {
    if (!antigravityQuota?.models.length) return null;
    const total = antigravityQuota.models.reduce((sum, m) => sum + (m.remainingFraction ?? 0), 0);
    return Math.round((total / antigravityQuota.models.length) * 100);
  }, [antigravityQuota]);

  const activeAccount = activeSession
    ? accounts.find((account) => account.id === activeSession.accountId)
    : undefined;
  const orderedAccounts = sortByLatestActivity(accounts);

  const providerGroups = useMemo<ProviderGroup[]>(() => {
    return [
      {
        id: 'codex',
        title: 'Codex',
        meta: 'CLI',
        accounts: orderedAccounts,
      },
      {
        id: 'antigravity',
        title: 'Antigravity',
        meta: 'IDE',
        accounts: [],
      },
    ];
  }, [orderedAccounts]);

  const activeGroup =
    providerGroups.find((group) => group.id === activeProviderId) ?? providerGroups[0];

  /* eslint-disable react-hooks/set-state-in-effect -- derived state fallback */
  useEffect(() => {
    if (!providerGroups.some((group) => group.id === activeProviderId)) {
      setActiveProviderId(providerGroups[0]?.id ?? 'codex');
    }
  }, [activeProviderId, providerGroups]);
  /* eslint-enable react-hooks/set-state-in-effect */

  useEffect(() => {
    if (!inTauri()) return;
    const panel = panelRef.current;
    if (!panel) return;

    const sync = () => scheduleTrayPopoverWindowSize(panel);
    sync();

    const observer = new ResizeObserver(sync);
    observer.observe(panel);

    const unlistenRefresh = listen('quota-popover-refresh', sync);
    const unlistenShow = getCurrentWebviewWindow().listen('tauri://focus', sync);

    return () => {
      observer.disconnect();
      void unlistenRefresh.then((fn) => fn());
      void unlistenShow.then((fn) => fn());
    };
  }, [accounts.length, providerGroups.length, activeSession?.id, status]);

  useEffect(() => {
    scheduleTrayPopoverWindowSize(panelRef.current);
  }, [accounts.length, quotas.length, activeGroup?.accounts.length, antigravityQuota?.models.length]);

  return (
    <div ref={panelRef} className="trayPopoverPanel" data-theme={resolvedTheme}>
      <TrayPopoverHeader
        status={status}
        refreshing={refreshing || refreshingAntigravity}
        onRefresh={() => void handleRefreshAll()}
        onClose={() => void closePopover()}
        accountsWithQuotaData={isAntigravityActive ? antigravityUsableCount : accountsWithQuotaData}
        totalAccounts={isAntigravityActive ? antigravityModelsCount : (accounts.length || 0)}
        avg5hRemaining={isAntigravityActive ? antigravityAvgRemaining : avg5hRemaining}
        availableQuotaAccounts={isAntigravityActive ? antigravityUsableCount : availableQuotaAccounts}
        activeAccountDisplayName={activeAccount?.displayName ?? activeSession?.accountId}
        activeSessionId={activeSession?.id}
        resolvedTheme={resolvedTheme}
        onToggleTheme={handleToggleTheme}
        activeProviderId={activeProviderId}
      />

      <TrayAccountList
        providerGroups={providerGroups}
        activeProviderId={activeProviderId}
        setActiveProviderId={setActiveProviderId}
        quotas={quotas}
        activeSession={activeSession}
        relayingAccountId={relayingAccountId}
        refreshingQuotaIds={refreshingQuotaIds}
        onRefreshAccount={(account) => void refreshAccountQuota(account)}
        onRelayTo={(account) => void relayTo(account)}
        isDark={resolvedTheme === 'dark'}
        antigravityQuota={antigravityQuota}
      />

      <TrayPopoverFooter onClose={() => void closePopover()} onOpen={() => void openMain()} />
    </div>
  );
}
