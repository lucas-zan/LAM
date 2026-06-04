import type { CodexAccount, OperationPlan, SyncPlan, SyncRequest, SyncResult } from "../lib/types";
import { PlanView } from "./plan-view";
import { UIButton } from "./ui-button";

function accountLine(account: CodexAccount | undefined) {
  if (!account) return "—";
  const provider = account.providerId ?? "unknown";
  const kind = account.isRelay ? "relay" : "profile";
  return `${account.codexHome} · ${provider} · ${kind}`;
}

function mismatchNotice(from: CodexAccount | undefined, to: CodexAccount | undefined, planWarnings: string[]) {
  const fromPlan = planWarnings.find((w) => w.toLowerCase().includes("provider mismatch"));
  if (fromPlan) return fromPlan;
  if (!from || !to) return null;
  if (from.providerId && to.providerId && from.providerId !== to.providerId) {
    return `Provider mismatch: source uses ${from.providerId}, target uses ${to.providerId}. Resume can continue transcript context, but runtime behavior may differ.`;
  }
  if (!from.providerId || !to.providerId) {
    return "Provider mismatch check is incomplete because one side has unknown provider.";
  }
  return null;
}

export function SyncModal(props: {
  accounts: CodexAccount[];
  syncReq: SyncRequest;
  setSyncReq: (req: SyncRequest) => void;
  plan: OperationPlan | SyncPlan | null;
  syncResult: SyncResult | null;
  onDryRun: () => void;
  onExecute: () => void;
  onClose: () => void;
}) {
  const { accounts, syncReq, setSyncReq, plan, syncResult, onDryRun, onExecute, onClose } = props;
  const from = accounts.find((a) => a.id === syncReq.fromProfileId);
  const to = accounts.find((a) => a.id === syncReq.toProfileId);
  const warnings = plan && "warnings" in plan ? plan.warnings : [];
  const mismatch = mismatchNotice(from, to, warnings);
  const canExecute = Boolean(plan);

  function patchReq(patch: Partial<SyncRequest>) {
    setSyncReq({ ...syncReq, ...patch });
  }

  return (
    <>
      <div className="syncRoute">
        <RouteBox
          label="From"
          accountId={syncReq.fromProfileId}
          accounts={accounts}
          onChange={(id) => patchReq({ fromProfileId: id })}
          account={from}
        />
        <div className="syncRouteArrow" aria-hidden="true">
          →
        </div>
        <RouteBox
          label="To"
          accountId={syncReq.toProfileId}
          accounts={accounts}
          onChange={(id) => patchReq({ toProfileId: id })}
          account={to}
        />
      </div>

      <label className="syncOption">
        <input type="checkbox" checked={syncReq.syncSessions} onChange={(e) => patchReq({ syncSessions: e.target.checked })} />
        <span>
          <strong>Sync sessions/ for resume </strong>
          <span>Mirrors session transcripts only. Required for codex resume.</span>
        </span>
      </label>

      <label className="syncOption">
        <input
          type="checkbox"
          checked={syncReq.backupTargetSessions}
          onChange={(e) => patchReq({ backupTargetSessions: e.target.checked })}
        />
        <span>
          <strong>Backup target sessions/ before copy </strong>
          <span>Creates sessions.backup.&lt;timestamp&gt; under the target CODEX_HOME.</span>
        </span>
      </label>

      <label className="syncOption">
        <input
          type="checkbox"
          checked={syncReq.sidecarBackupHistory}
          onChange={(e) => patchReq({ sidecarBackupHistory: e.target.checked })}
        />
        <span>
          <strong>Sidecar backup history only </strong>
          <span>Optional history.from-&lt;source&gt;.jsonl. Target history.jsonl is never merged.</span>
        </span>
      </label>

      {mismatch ? <div className="notice warn">{mismatch}</div> : null}

      {!to?.isRelay && to ? (
        <div className="notice warn">Target is a primary profile; a relay workspace is recommended for session relay.</div>
      ) : null}

      <div className="previewBox">
        <div className="previewLine">
          <span>Always excluded</span>
          <strong>auth.json, API keys</strong>
        </div>
        <div className="previewLine">
          <span>Always excluded</span>
          <strong>config.toml, sqlite, cache, tmp, logs</strong>
        </div>
        <div className="previewLine">
          <span>History merge</span>
          <strong>Not supported in Phase 1</strong>
        </div>
      </div>

      <PlanView plan={plan} />

      {syncResult ? (
        <div className="result">
          Copied <strong>{syncResult.copied}</strong>, skipped <strong>{syncResult.skipped}</strong>.
          <div className="mono manifestLine">Manifest: {syncResult.manifestPath}</div>
        </div>
      ) : (
        <p className="syncHint">Dry-run first. Execute stays disabled until a plan is generated.</p>
      )}

      <div className="modalFoot">
        <UIButton type="button" variant="ghost" onClick={onClose}>
          Cancel
        </UIButton>
        <div className="modalFootPrimary">
          <UIButton type="button" variant="default" onClick={onDryRun}>
            Dry Run
          </UIButton>
          <UIButton type="button" variant="primary" disabled={!canExecute} onClick={onExecute}>
            Confirm Execute
          </UIButton>
        </div>
      </div>
    </>
  );
}

function RouteBox(props: {
  label: string;
  accountId: string;
  accounts: CodexAccount[];
  onChange: (id: string) => void;
  account?: CodexAccount;
}) {
  return (
    <div className="routeBox">
      <span className="routeBoxLabel">{props.label}</span>
      <select
        className="routeSelect"
        value={props.accountId}
        onChange={(e) => props.onChange(e.target.value)}
        aria-label={`${props.label} account`}
      >
        {props.accounts.map((account) => (
          <option key={account.id} value={account.id}>
            {account.displayName}
            {account.isRelay ? " (relay)" : ""}
          </option>
        ))}
      </select>
      <strong className="routeBoxName">{props.account?.displayName ?? props.accountId}</strong>
      <div className="routeBoxPath mono">{accountLine(props.account)}</div>
    </div>
  );
}
