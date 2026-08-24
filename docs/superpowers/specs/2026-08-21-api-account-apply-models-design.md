# API Account Apply Fetched Models

**Status**: Approved  
**Date**: 2026-08-21

## Problem

Fetch models in Edit API Account is discovery-only. Switch Model and Codex `/model` only use the saved provider allowlist, so newly fetched models cannot be selected until the user edits the Provider elsewhere.

## Goals

After a manual fetch in **Edit API Account**:

1. If fetched model ids match the saved allowlist (set equality, ignore order/labels): show a match hint; do **not** offer apply actions.
2. Otherwise offer:
   - **Replace all** — candidate allowlist = all fetched models
   - **Customize selection** — full checklist (saved models checked by default; can uncheck to remove; can check fetched models to add); confirm → candidate allowlist
3. If current `selectedModel` is not in the candidate allowlist, user **must** pick a default before Apply.
4. Apply persists allowlist + selected model; Switch Model and Codex `/model` both see the new set.

## Non-goals

- Do not change `refresh_provider_models` to persist.
- Do not merge Save URL/Key with Apply models into one forced save.
- Do not auto-select every fetched model on fetch.

## Design

### UI (Edit API Account)

- Fetch remains read-only preview.
- On mismatch: **Replace all** / **Customize selection**, then **Apply models**.
- On match: status hint only.
- **Save API account** continues to update base URL / optional API key only (no `models`).

### Backend

Extend `UpdateApiAccountConnectionRequestV2` with optional:

- `models?: { id, label }[]`
- `selectedModel?: string`

When `models` is present:

- non-empty, valid slugs, unique ids
- `selectedModel` required and ∈ `models`
- update provider `models` + `defaultModel`
- rebind using `selectedModel` (rewrites `models.json` / config projection)

When `models` is absent: existing URL/Key behavior unchanged.

### Consistency

Allowlist source of truth remains provider `models`. Codex catalog is rewritten on rebind. Switch Model reads provider models from the store after refresh.
