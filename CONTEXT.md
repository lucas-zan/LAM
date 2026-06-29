# LAM Usage Context

This context defines the project-specific language for LAM usage accounting and quota display. It separates local replayed activity from upstream Codex account snapshots so UI numbers can be compared without mixing sources.

## Language

**LAM Local Usage**:
Usage activity derived from local Codex session/event files and stored in LAM's SQLite read model. It is the source for calls, threads, local token activity, and the activity heatmap.
_Avoid_: session stats, local stats

**Codex Account Usage**:
Account-level usage reported by Codex upstream account APIs. It is the source for headline lifetime, peak, longest task, current streak, and longest streak values.
_Avoid_: account stats, Codex stats

**Usage Parity Delta**:
The difference between Codex Account Usage lifetime tokens and LAM Local Usage total tokens for the same visible scope. It shows whether local replayed usage is under or over the upstream account total.
_Avoid_: mismatch, gap

**Reset Credit**:
A Codex account credit that can reset a rate-limit window. The known account rate-limits API proves the available count, but expiry is only known if a probed authenticated endpoint exposes a stable expiry field or the operator provides a local manual expiry override.
_Avoid_: quota dot, reset dot
