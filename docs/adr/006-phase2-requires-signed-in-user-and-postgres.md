# ADR-006 — Phase 2 features require a signed-in user and Postgres; anonymous and in-memory paths are for demo and development

**Status:** Proposed (awaiting product-owner acceptance) · **Date:** 2026-09-20 · **Sprint:** team sprint A (`docs/PROGRESS.md`) · **Concept:** `docs/PERSONAL_AI_OS.md` §11, §12.2; `CLAUDE.md` rules 4–6

## Context

Authentication and persistence are optional in the runtime. `DATABASE_URL` and `JWT_SECRET` may be
unset; then sessions are in-memory, every visitor is anonymous with a generated user id, and the
Release R1 services fall back to process-lifetime stores (`PermissionServices::in_memory`,
`MemoryService::in_memory`, `SessionStore::new`). That is what keeps the marketing demo and the
offline test group working, and `CLAUDE.md` rule 6 requires it to stay.

Phase 2 is built on data that must outlive a process: the activity ledger, baselines, memory with
provenance, pending actions, audit and consents. Without a database they are lost on restart and can
never reach the 14-day baseline warm-up. Without a signed-in user, personal data is attached to an
anonymous id that nobody can come back to.

Two identifier types also coexist: `users.id` is a `UUID`, while adk sessions, `ui_sessions` and every
Phase 2 table key `user_id` as `TEXT` (the stringified UUID for signed-in users, a generated id for
anonymous ones). The R1 review flagged this as needing a decision.

## Decision

1. **A Phase 2 deployment requires `DATABASE_URL` and `JWT_SECRET`.** The `deploy/` compose stack
   sets both. When either is missing the server still boots (rule 6) but logs one warning at startup,
   `Phase 2 persistence disabled — demo/dev mode`, and `/health` already exposes `postgres_enabled`
   and `auth_enabled` for the UI to label the state. A `AGENTRIX_REQUIRE_PERSISTENCE=1` flag makes the
   missing configuration a boot failure; the production compose file sets it. Follow-up ticket for
   team sprint B (Platform).
2. **The anonymous path is the Phase 1 tour.** `create_session`, `submit_intent`, the scenario
   workflows and `?demo=1` stay anonymous so the public demo keeps working. Every route that acts,
   changes settings or reads personal data is `known` (already true for the R1 routes: actions,
   permissions, pause, memory, chat). Anything an anonymous session stores in the R1 in-memory
   services is per-process and disposable, and the UI labels it as simulated (NFR-008). Surface task.
3. **`user_id` is `TEXT` everywhere and stays that way.** Signed-in users are identified by the
   string form of `users.id`; anonymous sessions by their generated id. No migration converts the
   Phase 2 tables to `UUID`: adk-session identifies users by string, anonymous ids are not UUIDs, and
   R1 already committed to `TEXT` in migrations 005–007. Joins to `users` cast on the `users` side
   (`users.id::text`).
4. **Tests keep three groups.** Unit and offline tests never need a database; Postgres tests are
   named so the CI filter picks them up and they run against the CI service container; live tests
   need secrets. No test in the offline group may depend on `DATABASE_URL`.

## Consequences

- No hidden requirement is added to compile or boot; the requirement is a deployment gate, which is
  where the plan's beta lives.
- Kim's and Jotham's stores follow the R1 pattern: in-memory `Inner` plus optional `PgPool`, loaded
  per `user_id` on first access.
- The R1 validation script's "offline" section remains valid: the Mother still orchestrates and the
  gate still decides without a database; only durability is missing.
- The Surface lane shows a "simulated / not persisted" label when `/health` reports
  `postgres_enabled: false`, and a "sign in to keep this" prompt on anonymous sessions.
- `docs/PROGRESS.md` §9 item "unify `user_id` type" is closed by this decision rather than by a
  migration.
