# ADR-005 — Tenancy: one Agentrix OS instance per person through the beta

**Status:** Proposed (awaiting product-owner acceptance) · **Date:** 2026-09-20 · **Sprint:** team sprint A (`docs/PROGRESS.md`) · **Concept:** `docs/PERSONAL_AI_OS.md` §1.2, §12.1, §12.2; `docs/SPRINT_PLAN.md` §9

## Context

The Phase 2 plan talks about a public beta, cost per active user, onboarding "on a fresh account" and
cross-user leakage as a threat. The runtime that Release R1 builds on is single-tenant in three places:

- **Integrations.** MCP children are spawned once at boot (`src/tools/mcp.rs`). Their credentials are
  operator-level: OAuth is completed by running each MCP binary's own auth command
  (`mcp-calendar auth google`, `mcp-email auth gmail`) and the tokens live in the operator's config
  directory. No `user_id` reaches any MCP call. `GET /api/oauth/{provider}` is a guide for the operator,
  not a per-user flow.
- **Scheduling.** Each ambient agent runs one cron per process (`src/ambient/service.rs`).
- **Global switches.** The DND flag (`AmbientStore`) and the pause switch (`PermissionStore`) are
  process-wide; the store's own comment says "the prototype serves one person".

At the same time the data model is already multi-user ready: a `users` table, JWT cookies, per-user
artifact paths, and `user_id` on every Phase 2 table (ledger, permissions, pending actions, audit,
memory, consents).

Making the service multi-tenant would add a workstream the plan does not schedule: MCP children (or
per-call credentials) per signed-in user, per-user cron loops, per-user pause and DND, and a tenant
check on every process-wide handle. Estimated at one to two team sprints, and it would have to land
before S4/S5 because the Work and Home worlds spawn identity-specific MCP children.

## Options

| | Option | Cost before R2 | What it buys |
|---|---|---|---|
| A | **One instance per person.** Each user gets their own process and Postgres database, provisioned from the existing `deploy/` compose stack. | None | Matches "one person → one AI OS"; integrations, cron and pause stay as they are |
| B | Multi-tenant service; MCP children spawned per signed-in user with that user's tokens; per-user cron and pause. | 1–2 sprints, before S4 | One hosted service, cheaper per user at scale |
| C | Multi-tenant service; MCP servers accept per-call credentials. | Changes in the `mcp-servers` monorepo plus B's per-user scheduling | Fewer processes than B |

## Decision

**Option A through the beta gate (end of team sprint F).** Agentrix OS is deployed as one instance per
person. Everything that is naturally per-process (MCP credentials, cron, DND, pause) may assume a
single owner.

Rules that keep option B open:

1. Persistent rows keep their `user_id` column and every query filters by it (already the R1 rule).
2. New process-wide handles (`OnceLock` services) are acceptable, but any state they hold that would
   differ between people must be keyed by `user_id`.
3. An instance has an **owner allowlist** (`AGENTRIX_OWNER_EMAILS`, comma-separated). Sign-in for any
   other account is refused, so a shared Google OAuth client cannot create a second user on someone
   else's instance. Follow-up ticket for team sprint B (Platform).
4. Onboarding (§13.8) is "provision an instance, then connect accounts" — the MCP auth commands are
   wrapped in one script (`scripts/connect_accounts.sh`, sprint B) instead of being typed by hand.

Revisit at the beta gate with real numbers: instances per operator, cost per instance, and whether
the MCP servers have gained per-call credentials in the meantime. A change supersedes this ADR.

## Consequences

- **S5-T6 "separate identities"** means two operator credential sets per instance (work Google
  account, personal Google account), each with its own MCP calendar/email children. Not per user.
- No per-user credential injection work in team sprint B; that time goes to the contacts store and
  the finance/health guardrails as planned.
- Metrics "per active user" in `SPRINT_PLAN.md` §10 are per instance.
- The threat "cross-user leakage" (§12.1) becomes "cross-instance leakage" and is an infrastructure
  concern (one database per instance, no shared volumes). The `user_id` filters remain as defence in
  depth.
- AWP external callers are unaffected: trust levels still apply, and the `known` level maps to the
  instance owner.
- `docs/PERSONAL_AI_OS.md` §12.2 "tokens injected per user" is deferred; the row should read "per
  instance" until this ADR is superseded.
