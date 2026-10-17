# ADR-014 — Missions and the proactive engine run on adk durable workers and the durable scheduler

**Status:** Proposed · **Date:** 2026-10-17 · **Milestone:** OS2 · **Concept:** `docs/AGENTIC_OS.md` §4.6–4.7, §10.5

## Context

The prototype's ambient agents ran on in-process cron, swallowed errors and published invented
results. Autonomous agents need goals, budgets, approvals that can wait, crash safety and a polite
way to reach the person.

## Decision

- A **mission** is `{goal, agents, cadence | trigger, budget, mode, expires}` created by the Mother
  (and approved by the person). Mission steps run on adk `DurableWorker` (fenced leases, heartbeats,
  crash resume, `AwaitingApproval` parking, no duplicate non-idempotent calls) and are fired by the
  `DurableScheduler` exactly once per occurrence, all on Postgres.
- **Triggers** (cron, app events from the bus, conditions) start background tasks the same way.
- An **attention manager** in the Mother decides how every ProActive item reaches the person:
  interrupt, lane, digest or drop — under budgets, quiet hours, focus and feedback, always with a reason.
- Spend caps and the kill switch use adk `SpendLedger` and `GovernanceControl`.

## Consequences

- J7 and J8 (missions that survive restarts and crashes) become testable.
- Run lineage across processes needs a parent/child link upstream in adk-rust.
- No background result is ever shown without a successful run behind it.
