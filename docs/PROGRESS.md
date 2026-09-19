# Agentrix Personal AI OS — Team Progress (Phase 2, Releases R2–R4)

**Companion to:** [`SPRINT_PLAN.md`](./SPRINT_PLAN.md) (task ids) · [`PERSONAL_AI_OS.md`](./PERSONAL_AI_OS.md) (concept) · [`IMPLEMENTATION_PLAN.md`](./IMPLEMENTATION_PLAN.md) (Phase 1 and R1 status)
**Status:** Draft v1.0 — 2026-09-19 · **Kickoff:** TBD (team sprint A = weeks 1–2 from kickoff)
**Scope:** Release R1 (S0–S3) is on `main` (PR #2) with product-owner validation pending. This document splits the remaining plan sprints S4–S12 across the team and tracks progress per person.

Tick the **Status** cell in each table as work lands (⬜ not started · 🟡 in progress · ✅ done).

---

## 1. How the plan is run

The plan sprints S4–S12 do not run as one line. Two tracks run side by side, with three shared lanes feeding both. This brings Releases R2 and R3 in together in six team sprints instead of nine.

### Lanes and ownership

| Lane | Owner | Owns | Directories (sole reviewer for changes) |
|---|---|---|---|
| Platform and data · tech lead · product owner | **James** | Migrations, stores, routes, tenancy and per-user credentials, consent persistence, retention, CI and deploy, validation sign-off | `src/state.rs`, `src/routes/`, `migrations/`, `src/permissions/store.rs`, `src/config.rs`, `business.toml`, `mcp_allowlists.toml`, `src/events/sse.rs` |
| Agents and worlds | **Kim** | Work and Home mothers, re-homing existing agents, agent bus, arbitration, briefing workflow, recipe detector | `src/mother/`, `src/worlds/`, `src/agents/`, `src/briefing/` |
| Surface | **Robert** | Chat panel, approvals inbox, domain colours and mode badges, TODAY, trust center, onboarding, mobile | `web/` |
| Intelligence | **Jotham** | Patterns, baseline, balance, digital behaviour, knowledge graph, phrasing lint, nightly jobs | `src/intelligence/`, `scripts/synth_ledger.py`, `src/bin/load_ledger.rs` |
| Product and quality | *fifth seat, unfilled* | Validation recordings, journey fixtures, Playwright, security review, importers | `tests/`, `docs/`, `scripts/` |

Role assumptions: James is tech lead and product owner; Kim built R1 and keeps Agents and Worlds; Robert takes Surface; Jotham takes Intelligence. Swap people between lanes if that is wrong, but keep the ownership boundaries — they are what stop five people colliding in the same files.

### Sprint key

Team sprints are **A–F**, two weeks each. Plan sprint and task ids (`S4-T3`) are from `SPRINT_PLAN.md`.

| Team sprint | Worlds track (Kim) | Intelligence track (Jotham) | Command center and trust (Robert, James) |
|---|---|---|---|
| **A** | S4 Work World | S7 patterns + baseline | S10-T3/T4 pulled forward (chat panel, approvals inbox) |
| **B** | S5 Home World | S7 phrasing, jobs, observations, confirmation | S11-T2 pulled forward (memory panel) |
| **C** | S6 bus, arbitration, journeys, briefing | S8 balance, conflicts, behaviour | briefing card, observation feed |
| **D** | S6 tail; S9 consumers (Career, Research) | S9 reading + knowledge graph | knowledge view, TODAY skeleton |
| **E** | S12 recipe detector, AWP ceiling | S12 pattern signals, metrics | S10 TODAY, S11 permissions panel |
| **F** | S12 home recipes, onboarding defaults | tuning, redaction audit | S11 trust center, S12 onboarding, beta gate |

The only hard cross-track dependency is that observations surface inside the briefing (team sprint C). Until then the Intelligence lane exposes them through its own API and the Surface lane shows them in a feed.

### Release gates

| Release | Team sprint | Gate |
|---|---|---|
| R1 Foundation | before A | R1 validation script in `IMPLEMENTATION_PLAN.md` recorded and signed off (open) |
| R2 Two Worlds | end of C | "two worlds, one briefing" demo (`SPRINT_PLAN.md` §5) |
| R3 Intelligence | end of D | "it noticed my routine changed" demo (§6) |
| R4 Command center and trust · public beta | end of F | beta gate script (S12 validation) |

---

## 2. Team sprint status

| Team sprint | Dates | James | Kim | Robert | Jotham | Tag on completion |
|---|---|---|---|---|---|---|
| A | TBD | 🟡 | ✅ | ⬜ | 🟡 | `v1.4.0-p2` |
| B | TBD | ⬜ | ⬜ | ⬜ | ⬜ | `v1.5.0-p2` |
| C | TBD | ⬜ | ⬜ | ⬜ | ⬜ | `v1.6.0-p2` (R2) |
| D | TBD | ⬜ | ⬜ | ⬜ | ⬜ | `v1.9.0-p2` (R3) |
| E | TBD | ⬜ | ⬜ | ⬜ | ⬜ | `v1.11.0-p2` |
| F | TBD | ⬜ | ⬜ | ⬜ | ⬜ | `v2.0.0-beta` (R4) |

---

## 3. James — Platform and data, tech lead, product owner

Owns migrations, stores, routes, the SSE enum, the manifest and the allowlist file. Signs off every validation script.

**Migration numbering.** R1 used 004 (chat history), 005 (ledger), 006 (permissions), 007 (memory), so every number in `SPRINT_PLAN.md` from S4 onward shifts. Use this table to avoid collisions:

| Migration | Table(s) | Plan task | Team sprint |
|---|---|---|---|
| `008_tasks.sql` | `tasks` | S4-T3 | A |
| `009_contacts.sql` | `contacts` | S5-T3 | B |
| `010_baselines_observations.sql` | `activity_daily`, `baselines`, `observations` | S7-T1 | A (for Jotham) |
| `011_boundaries.sql` | `boundaries` | S8-T3 | C |
| `012_knowledge.sql` | `reading_items`, `knowledge_nodes`, `knowledge_edges` | S9-T2 | D |
| `013_retention.sql` | `retention_policies` | S11-T5 | D |
| `014_recipes.sql` | `automation_recipes` | S12-T1 | E |

| Team sprint | Plan tasks | What ships | Hand-off | Status |
|---|---|---|---|---|
| **A** | S0 follow-ups · S4-T3 · S11-T4 · S7-T1 migration | **ADR-005 tenancy** (one instance per person vs. multi-user with per-user MCP credentials and per-user scheduling) and **ADR-006 prerequisites** (Phase 2 requires login + Postgres; anonymous/in-memory path = demo only). Tasks migration 008 and task tools. Consent persistence pulled forward from S11 (replaces `InMemoryConsentService`). Migration 010 for Jotham. **Run and sign off the R1 validation script.** | Tenancy answer decides his own and Kim's sprint B | 🟡 PR #6 (ADRs, CI) and PR #9 (tasks, consents, migration 010) open; R1 offline validation run on #9, dev-sign-in and live parts pending |
| **B** | S5-T3 migration · S5-T6 · S5-T7 boot check · S5-T8 | Contacts migration 009. Separate work and home OAuth identities. Per-user credential injection into MCP children if ADR-005 says multi-tenant. Finance allowlist read-only boot check. HealthKit importer (moves to the fifth seat when filled). | Family agent to Kim | ⬜ |
| **C** | S6-T6 route · S6-T7 · S8-T3 | Briefing route `GET /api/briefing/today`, `briefing` SSE event, briefing cron wiring. Boundaries migration 011 and `GET/PUT /api/boundaries`. **R2 sign-off.** | Briefing payload to Robert and Kim | ⬜ |
| **D** | S9-T2 · S11-T5 · S7-T5 review | Knowledge migration 012. Retention policies 013 and nightly purge job. Review Jotham's `observation` event into `FieldEvent`. **R3 sign-off.** | Knowledge API to Robert | ⬜ |
| **E** | S10-T1 · S11-T3 · S12-T1 · S12-T3 · S12-T6 | `GET /api/today` payload. Audit route. Recipes migration 014, model and API. Recipe runner with the never-automate rule (`publish_public`, `financial`) in the gate. Token budgets and model routing. | TODAY payload to Robert | ⬜ |
| **F** | S11-T5 account · S11-T6 · S11-T7 · S12-T8 · S12-T9 | `DELETE /api/account` export-then-delete. Encrypt OAuth tokens, key rotation runbook. Security review and prompt-injection tests. Regression suite on the production URL. **Beta gate.** | | ⬜ |

---

## 4. Kim — Agents and worlds

Owns the mother, worlds, agents and briefing modules. Re-homing is metadata on existing agents (world, mode, memory scope in `mcp_allowlists.toml`), not file renames, until after R2 — the Phase 1 demo tour must keep working.

| Team sprint | Plan tasks | What ships | Hand-off | Status |
|---|---|---|---|---|
| **A** | S4-T1 · S4-T2 · S4-T4 · S4-T5 · S4-T6 · S4-T7 · S4-T9 | `work_mother` parallel fan-out returning one structured result. Work agents tagged and scoped (productivity, email, team_comms, project, research_knowledge, work_automation). Email follow-up tracker from hashed subjects. Career and Professional Social as **labeled stubs** (BK-101). Allowlist entries with effects and default modes. Tests: fan-out ≥ 3 results, 3-day-old thread flagged, no finance/health tools in any work agent. | S4-T3 landed in PR #9: Productivity inherits the `tasks` toolset through `calendar_agent`; the morning calendar adapter still fills the cards | ✅ merged in PR #5 |
| **B** | S5-T1 · S5-T2 · S5-T3 agent · S5-T4 · S5-T5 · S5-T7 prompt · S5-T9 | `home_mother`. Home agents re-homed (finance, health_wellness, entertainment, social_fun, travel). Family agent (calendar id from memory, imported contacts, important dates) and Personal Productivity agent. Personal Social labeled stub (BK-102). Health prompt guardrail (no diagnosis vocabulary, escalation sentence). Tests: family routing, finance has zero non-read tools, health lint blocks a diagnosis, identity separation. | Contacts from memory + `CONTACTS_CSV_PATH` until migration 009. Personal Productivity reads the shared `tasks` store (S4-T3) and carries its five tools, scoped to home | 🟡 `phase2/S05-home-world` (PR #7, after #5) |
| **C** | S6-T1 · S6-T2 · S6-T3 · S6-T4 · S6-T5 · S6-T8 · S6-T9 | `AgentMessage` bus with trace ids, fan-out depth cap 2, timeouts. Mothers publish and consume. Journey §13.5 (email → availability → prep → one recommendation → approve sends and books). Journey §13.3 overlap check. Arbitration v1 (protected time, dedupe into one question). Voice reads the briefing. Tests: bus round-trip, six-section briefing with empty states, conflict fixture yields one question and two pending actions. | Conflict observations arrive from Jotham (S8-T2); S6-T3 waits on journey fixtures (fifth seat) | 🟡 bus, arbitration, overlap check, voice in `phase2/S06-bus-arbitration` |
| **D** | S7-T6 · S9-T4 · S9-T5 | Mother `read_observations` tool; observations surface only in the briefing or on "how am I doing?", never mid-task. `reading_knowledge` agent and `query_knowledge` Mother tool (§13.6). Career consumes rising topics; Research schedules briefs on them (Observe). | Needs knowledge API from Jotham and James | ⬜ |
| **E** | S12-T2 · S12-T5 | Repetition detector (recurring `subject_hash`, approval streaks ≥ N) proposing recipes as `recommended` memory plus a pending approval showing exact condition and effect. Typed A2A messages to the Mother; external callers capped at Suggest regardless of user modes; opt-in `observation.created` subscription. | Pattern signals from Jotham | ⬜ |
| **F** | S12-T4 · S12-T7 server side | Home recipes (household tasks, shopping list, morning audio, reminders only for Health). Onboarding defaults: every agent starts in Observe; 14-day warm-up notice. Journey fixes from the beta gate. | | ⬜ |

---

## 5. Robert — Surface

Owns `web/`. Wires behaviour behind the existing field UI without restyling it (CLAUDE.md rule 7). R1 shipped **no UI**, so sprint A starts against routes that already exist on `main`: `/api/sessions/{sid}/chat`, `/api/actions`, `/api/permissions`, `/api/memory`, `/api/sessions/{sid}/events`.

| Team sprint | Plan tasks | What ships | Hand-off | Status |
|---|---|---|---|---|
| **A** | S4-T8 · S2-T2 client · S10-T3 v0 · S10-T4 v0 · BK-110 | Domain colour and mode badge on agent tiles and cards (from `card_spawn.domain` / `mode`). Content-free UI event posts (focus/blur, card opens per domain, notification counts). Chat panel v0 against the chat route (multi-turn, reuses `suzy_summary`/`card_*`/`suggest`/`done`). Approvals inbox v0 against the actions route (approve/reject/edit, batch; consumes `permission_request` and `action_result`). TypeScript types generated from `FieldEvent`. | Both routes exist today | 🟡 |
| **B** | S11-T2 v0 · S5-T3 rail · S8-T5 | Memory panel v0 (known / assumed / recommended tabs; confirm, correct, forget; provenance). Family rail populated from the Family agent. Focus and notification hooks complete. | Memory route exists today | ⬜ |
| **C** | S6-T6 card · S7-T5 feed | Briefing card with six sections (Work / Home / Learning / Wellbeing / Balance / Suggested actions) and honest "not connected" empty states. Observation feed with accept, dismiss, snooze, correct. | Needs James's briefing route and Jotham's observations API | ⬜ |
| **D** | S9-T6 · S10-T2 skeleton | Knowledge view (topics with trend, related items and projects). TODAY layout skeleton (Appendix E) with lens toggle Both / Work / Home persisted in preferences. | Needs knowledge API | ⬜ |
| **E** | S10-T2 · S10-T5 · S10-T6 · S10-T7 · S11-T1 | TODAY complete from `GET /api/today`. Mobile ≤ 900 px layouts, intent input ≥ 16 px (NFR-009). `?demo=1` scripted TODAY fixture labeled "simulated" (NFR-008). Playwright smoke: load TODAY, approve one action, switch lens. Permissions panel (per-agent mode, per-tool overrides, per-world pause, quiet hours, global pause). | Needs TODAY payload from James | ⬜ |
| **F** | S11-T1 · S11-T2 · S11-T3 · S12-T7 UI · BK-111 | Trust center complete: permissions, memory, audit viewer with "what ran while you were away" and undo. Onboarding flow (§13.8): connect worlds, consents with purposes, default modes, warm-up notice. Playwright visual regression for TODAY and trust center. | | ⬜ |

---

## 6. Jotham — Intelligence

Owns `src/intelligence/` and the synthetic data scripts. Starts on day one against synthetic data because the ledger (`activity_events`), the generator (`scripts/synth_ledger.py`) and the loader (`src/bin/load_ledger.rs`) shipped in R1. Deterministic statistics first; the LLM only phrases results, under the §7.6 neutral-language lint.

| Team sprint | Plan tasks | What ships | Hand-off | Status |
|---|---|---|---|---|
| **A** | S7-T1 · S7-T2 · S7-T9 | Pattern aggregation over the ledger into `activity_daily (user_id, day, dimension, value)` for the §7.2 dimensions (migration 010 via James). Baseline: median + MAD, weekday/weekend classes, 28-day window, 14-day warm-up, drift rule (§8.2), 7-day cooldown, "meaningful" rule (≥ 2 dims or 1 dim ≥ 10 d). Tests on `synth_ledger.py --weeks 6 --drift`: exactly one drift observation; warm-up suppresses; cooldown suppresses repeats. | Migration numbering from James | 🟡 PR #10 — code + tests; CI run and Kim review pending |
| **B** | S7-T3 · S7-T4 · S7-T5 · S7-T7 · S7-T8 | Phrasing: facts → neutral text via LLM, **lint** rejects banned vocabulary and requires number + period + baseline reference, deterministic fallback template. Jobs: nightly full recompute + 30-min incremental on `AmbientAgent`/`CronTrigger`, respecting DND and quiet hours. `GET /api/observations`, accept/dismiss/snooze/correct, `observation` SSE event. Confirmation: "Is this your usual routine?" → `baselines.confirmed` + known memory `routine.*`; re-learn resets the window. Per-dimension toggles. | Observations API to Robert | ⬜ |
| **C** | S8-T1 · S8-T2 · S8-T4 · S8-T6 · S8-T7 | Balance: attention share, spillover after `work.end`, weekend work, postponement debt (`postponed_count ≥ 3`), family/social cadence vs baseline → `balance` observations. Conflict detection across both identities' calendars + protected time → `conflict` observations (replaces the S6-T4 check). Digital Behavior: context switching, notification load, long uninterrupted work > 120 min, distractions in focus blocks, social consumption vs baseline. Balance facts for the briefing. Tests: seeded spillover → one observation; seeded conflict → one question; lint blocks "too much"/"should". | Conflict observations to Kim's arbitration | ⬜ |
| **D** | S9-T1 · S9-T3 · S9-T7 | Reading events (news opens, research briefs, artifact opens, imports: browser reading list JSON, Kindle highlights CSV) → content-free `reading` ledger rows + `reading_items` under the `reading` consent. Nightly topic extraction (LLM, batched), weights = recency × frequency, edges to projects and goals; taxonomy ≤ 50 topics per user. Tests: imported items → topics → trend query; consent revoked → import refused. | Graph to Kim's agent and Robert's view | ⬜ |
| **E** | S12-T2 patterns · §10 metrics | Recurring `subject_hash` and approval-streak signals for the recipe detector. Instrument the metrics in `SPRINT_PLAN.md` §10 (p95 latencies, turns answered by ≥ 2 agents, approval/rejection rates, drift accepted vs dismissed, assumed items corrected). | Signals to Kim | ⬜ |
| **F** | S11-T8 · tuning | Redaction audit of intelligence output and tracing (no payload bodies or memory values). MAD floors per dimension and `k` tuning from dismissal data. Metrics dashboard. | | ⬜ |

---

## 7. The fifth seat — Product and quality (unfilled)

These tasks currently sit inside James's and Robert's tables. When the fifth person joins they move here and James's sprint F gets lighter.

| Team sprint | Plan tasks | What they take over |
|---|---|---|
| before A | R1 validation | Record the R1 validation script from `IMPLEMENTATION_PLAN.md` (offline part first; live part needs `GOOGLE_API_KEY` + `mcp-email`) |
| A–B | S5-T8 · ticketing | HealthKit importer; GitHub milestones A–F with one issue per task id |
| C | S6-T9 fixtures | Seeded email and calendar fixtures for journeys §13.3 and §13.5 |
| E | S10-T7 · BK-111 | Playwright smoke and visual regression |
| F | S11-T7 · S12-T9 | Security review, prompt-injection tests, beta gate run on desktop + mobile Safari |

---

## 8. Working rules

- **One task in progress per person.** Task ids are already ticket-sized; use them as issue titles with one milestone per team sprint.
- **Every task is a PR to `main` behind a flag.** Do not repeat the R1 pattern of four sprints landing in one branch — that is why R1 is still unvalidated. Runtime flags (Mother toggle, gate mode warn/enforce) keep the old path alive until the validation passes.
- **Review pairs.** Kim reviews Jotham and vice versa (both consume the ledger and memory). James reviews Robert for contract changes. The product owner reviews every PR against the validation checklist.
- **Shared files change through James only:** `src/state.rs`, `src/events/sse.rs`, `business.toml`, `mcp_allowlists.toml`, `migrations/`.
- **Definition of done** is unchanged from `SPRINT_PLAN.md` §1: code + tests, labeled stubs for gaps, every write path through the gate with an audit row, `user_id` + `domain` + retention rule on every new table, concept-doc section in the PR description.
- **Cadence (two weeks).** Day 1 planning across both tracks. 15 minutes daily. Day 7 integration checkpoint: all flags on, full Phase 1 tour run. Day 9 freeze. Day 10 validation recordings, sign-off, tag, update §2 above and the progress tables in `SPRINT_PLAN.md` / `IMPLEMENTATION_PLAN.md`.
- **Dev environment.** Sibling checkouts `../adk-rust` (on `main`) and `../mcp-servers`, built MCP binaries, `GOOGLE_API_KEY`, Postgres via `docker compose up -d`. Postgres is mandatory for development from sprint A onward.

---

## 9. Open items carried from the R1 review

| Item | Owner | Due |
|---|---|---|
| Camera channel — Gemini Live sees and hears: frames over `/ws/voice`, `ui_gesture` → swipe worlds / pause / briefing, flag `AGENTRIX_CAMERA`. Outside the S4–S12 plan; James to slot it (touches `routes/events.rs` kinds and the consent categories). | Kim | `phase2/camera-live-vision` |
| ADR-005 tenancy: one deployment per person vs. multi-user with per-user MCP credentials and per-user scheduling. Today MCP children are spawned once at boot with operator-level OAuth, DND is one global flag, and each ambient agent runs one cron per process. | James | Sprint A, week 1 — **Proposed** in PR #6 (`docs/adr/005-…`), awaiting acceptance |
| ADR-006 prerequisites: Phase 2 requires login + `DATABASE_URL`; define what the anonymous / in-memory path does (recommendation: demo only). Unify `user_id` type (existing `VARCHAR(255)` vs. `users.id UUID`). | James | Sprint A, week 1 — **Proposed** in PR #6 (`docs/adr/006-…`); `user_id` stays `TEXT` by decision |
| Consent persistence: `InMemoryConsentService` still bound in `main.rs`. | James | Sprint A — done in PR #9 (`src/memory/consent.rs`, `GET/PUT /api/consents`) |
| R1 validation script never run; S0–S3 marked "PO validation pending". | James (fifth seat when filled) | before Sprint A |
| `docs/personal-ai-os.html` links to the fork branch instead of upstream `main`. | Kim | ✅ fixed in `phase2/S04-work-world` |
| CI triggers only on `main` and `milestone/**` pushes; add `phase2/**` and `docs/**`. | James | Sprint A — done in PR #6 |
| `cargo run` stopped starting the server after R1 added `src/bin/load_ledger.rs` (README and CLAUDE.md still say `cargo run`). | James | Sprint A — fixed in PR #9 (`default-run`) |

---

*Update §2 and the Status cells as PRs merge. Keep task ids in commit messages and PR descriptions.*
