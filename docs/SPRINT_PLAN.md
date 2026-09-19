# Agentrix Personal AI OS — Sprint Plan (Phase 2)

**Companion to:** [`PERSONAL_AI_OS.md`](./PERSONAL_AI_OS.md) (concept & architecture)
**Baseline:** [zavora-ai/zavora-os](https://github.com/zavora-ai/zavora-os) @ `228ec78`
**Status:** Draft v1.0 — 2026-09-19
**Convention:** Phase 1 = milestones M0–M11 in `docs/IMPLEMENTATION_PLAN.md`. Phase 2 = sprints S0–S12 below. Sprint IDs, task IDs (`S4-T3`) and backlog IDs (`BK-1xx`) are stable references for tickets.

---

## 1. Planning assumptions

| Assumption | Value | Change it if… |
|---|---|---|
| Sprint length | 2 weeks (S0 is 1 week) | Team prefers 1-week cadence → split each sprint at the "Validation" line |
| Team | 1 tech lead / Rust backend · 1 Rust agents engineer · 1 frontend engineer · 0.5 product designer · product owner | Fewer people → keep sprint scope, extend duration; do not cut trust/privacy work |
| Total duration | 1 + 12 × 2 = **25 weeks** (~6 months) to public beta | — |
| Illustrative dates | Start Mon **2026-10-05** → beta gate Fri **2027-04-09** (includes a two-week holiday buffer) | Purely illustrative; shift all dates by the real start |
| Model provider | Gemini via adk-rust (as today); model routing per agent | Provider change is isolated in `adk-model` |
| Toolservers | Existing `mcp-servers` monorepo; new gaps tracked as BK items | — |
| Phase 1 state | Code contains M5–M11 features and `/health` reports milestone `M11`; the progress table in `IMPLEMENTATION_PLAN.md` still shows M4 in progress and is stale | Reconciled in S0 |

### Working each sprint (inherits the Phase 1 ritual)

1. Branch `phase2/Sxx-short-name`.
2. Complete **Tasks**; `cargo check`, `cargo test`, `cargo clippy` green; CI green.
3. Record the **Validation** script (screen capture) — the product owner runs it and signs off.
4. Merge to `main`, tag `v1.<sprint>.0-p2` (e.g. `v1.3.0-p2` after S3).
5. Update the **Progress** table below and `docs/IMPLEMENTATION_PLAN.md`.
6. Keep `business.toml` capabilities and `mcp_allowlists.toml` in sync with routes and agents (BK-003, BK-013 from Phase 1).

### Definition of done (per task)

- Code + unit tests; integration test in `tests/validate.rs` where the task touches routes, agents or the gate.
- No fabricated data: gaps ship as **labeled stubs** (existing `agents/stub.rs` pattern).
- Every new write path goes through the permission gate and writes an audit entry.
- Every new table has `user_id`, `domain` (where applicable) and a retention rule.
- Docs: concept doc section reference in the PR description.

---

## 2. Roadmap at a glance

```
Release R1 — FOUNDATION            Release R2 — TWO WORLDS          Release R3 — INTELLIGENCE          Release R4 — COMMAND CENTER & TRUST
 S0 Align & scaffold (1 wk)          S4 Work World                    S7 Patterns + Personal Baseline     S10 TODAY dashboard + Mother chat
 S1 Mother Agent v1                  S5 Home World                    S8 Balance + Digital Behavior       S11 Trust center + privacy hardening
 S2 Activity ledger + permissions    S6 Collaboration + Briefing v2   S9 Reading & Knowledge graph        S12 Automation recipes + AWP + beta
 S3 Personal memory v1
 ────────────────────────────────    ────────────────────────────     ────────────────────────────────    ────────────────────────────────
 Internal demo: "one brain,          Internal demo: "two worlds,      Internal demo: "it noticed          Public beta gate
 safe actions, real memory"          one briefing"                    my routine changed"
```

| Sprint | Weeks | Illustrative dates | Theme | Headline outcome |
|---|---|---|---|---|
| **S0** | 1 | Oct 5 – Oct 9 | Align & scaffold | Stale plan reconciled; domain model; module skeletons; synthetic data generator |
| **S1** | 2–3 | Oct 12 – Oct 23 | Mother Agent v1 | One orchestrator (intake → delegate → synthesize) behind chat and A2A; 7 scenarios still work |
| **S2** | 4–5 | Oct 26 – Nov 6 | Ledger + permission modes | Content-free activity ledger; Observe/Suggest/Automate gate at tool boundary; pending actions; audit |
| **S3** | 6–7 | Nov 9 – Nov 20 | Personal memory v1 | known/assumed/recommended store, API, tools, encryption at rest |
| **S4** | 8–9 | Nov 23 – Dec 4 | Work World | `work_mother` + 8 work agents (5 extended, 3 new/labeled) |
| **S5** | 10–11 | Dec 7 – Dec 18 | Home World | `home_mother` + 8 home agents (5 extended, 3 new/labeled); health & finance guardrails |
| **S6** | 12–13 | Jan 4 – Jan 15 | Collaboration + Briefing v2 | Agent message bus; two cross-agent journeys; full daily briefing on cron and voice |
| **S7** | 14–15 | Jan 18 – Jan 29 | Patterns + Baseline | Deterministic patterns; 14-day warm-up; drift detection; "is this your normal?" |
| **S8** | 16–17 | Feb 1 – Feb 12 | Balance + Behavior | Work/Home attention, boundaries, conflict detection; neutral phrasing lint |
| **S9** | 18–19 | Feb 15 – Feb 26 | Reading & Knowledge | Reading ledger; knowledge graph; Learning section; Career agent uses it |
| **S10** | 20–21 | Mar 1 – Mar 12 | Command center UI | TODAY dashboard; Work/Home lens; persistent Mother chat; approvals inbox; mobile |
| **S11** | 22–23 | Mar 15 – Mar 26 | Trust center + hardening | Permissions, memory, audit, consent UIs; retention jobs; security review |
| **S12** | 24–25 | Mar 29 – Apr 9 | Recipes + AWP + beta | Recipe engine; Mother as AWP entry; cost controls; onboarding; beta gate |

Holiday buffer: the illustrative calendar skips Dec 21 – Jan 1.

### Progress (update as sprints merge)

| Sprint | Branch | Status | Tag |
|---|---|---|---|
| S0 | `phase2/r1-foundation` | ✅ code + tests · PO validation pending | `v1.0.1-p2` |
| S1 | `phase2/r1-foundation` | ✅ code + tests · PO validation pending | `v1.1.0-p2` |
| S2 | `phase2/r1-foundation` | ✅ code + tests · PO validation pending | `v1.2.0-p2` |
| S3 | `phase2/r1-foundation` | ✅ code + tests · PO validation pending | `v1.3.0-p2` |
| S4 | `phase2/S04-work-world` | ⬜ | `v1.4.0-p2` |
| S5 | `phase2/S05-home-world` | ⬜ | `v1.5.0-p2` |
| S6 | `phase2/S06-collab-briefing` | ⬜ | `v1.6.0-p2` |
| S7 | `phase2/S07-baseline` | ⬜ | `v1.7.0-p2` |
| S8 | `phase2/S08-balance` | ⬜ | `v1.8.0-p2` |
| S9 | `phase2/S09-knowledge` | ⬜ | `v1.9.0-p2` |
| S10 | `phase2/S10-command-center` | ⬜ | `v1.10.0-p2` |
| S11 | `phase2/S11-trust-center` | ⬜ | `v1.11.0-p2` |
| S12 | `phase2/S12-recipes-beta` | ⬜ | `v2.0.0-beta` |

**Legend:** ⬜ not started · 🟡 in progress · ✅ done

> **Implementation notes (R1, 2026-09-19):** S0–S3 landed together on `phase2/r1-foundation` in one commit per sprint. Deviations from the plan: migrations are numbered 004 (chat history), 005 (activity ledger), 006 (permissions), 007 (memory) instead of 004–006; the Mother's LLM half (`mother_agent`) ships with internal tools while delegation is planned in code until the S6 bus; leaf agents receive scoped `read_memory` / `propose_memory` through the shared `filtered_for_agent` wrapper rather than per-builder edits; a first-boot compatibility fix tracks adk-rust `main` (rmcp 3, `AwpState::builder`).

---

## 3. Concept → sprint traceability

| Concept section (`PERSONAL_AI_OS.md`) | Sprints |
|---|---|
| §3 Mother Agent | S1, S6, S12 |
| §4 Work World | S4 |
| §5 Home World | S5 |
| §6 Agent contract & catalog | S0 (contract), S4, S5 |
| §7 Intelligence layer | S2 (ledger), S7, S8, S9 |
| §8 Personal Baseline | S7 |
| §9 Balance system | S8 |
| §10 Automation layer | S2 (gate, pending, audit), S12 (recipes) |
| §11 Memory architecture | S3, S11 (UI) |
| §12 Privacy & security | S2 (audit), S3 (encryption), S11 (consent, retention, review) |
| §13 User journeys | S6 (13.1, 13.3, 13.5), S7 (13.4), S9 (13.6), S10 (13.2), S12 (13.7, 13.8) |
| §14 Agent-to-agent protocol | S6 |
| §15 Future expansion | post-beta backlog |
| Appendix B API/SSE contract | S1, S2, S3, S6, S7, S10 |
| Appendix C data model | S2, S3, S4, S7, S9, S11, S12 |

---

## 4. Release R1 — Foundation (S0–S3)

**Release goal:** one brain, safe actions, real memory. After R1 the product still looks like today's field UI, but every intent goes through the Mother Agent, every write goes through the permission gate, and the system can remember with provenance.

**Release demo (end of S3):** "What's happening with work?" → one Suzy answer from three agents. Inbox agent in Suggest mode drafts two replies and queues them; approving one sends it and writes an audit row. "Remember I never take meetings before 10" → shows as *known*; "I think you usually start around 9:40" → shows as *assumed*.

---

### S0 — Align & scaffold (1 week)

**Goal:** Reconcile the plan with reality and lay the Phase 2 skeleton so S1–S3 can proceed with minimal merge conflicts.

**Why now:** The progress table in `docs/IMPLEMENTATION_PLAN.md` says M4 is in progress while the runtime reports `M11`; the team needs one truth before adding a second phase. The domain tag and agent contract touch shared types and must land first.

| Task | Detail | Files |
|---|---|---|
| **S0-T1** | Reconcile Phase 1: mark M5–M11 done/partial against code and CI; list any unrun validation gates as tickets | `docs/IMPLEMENTATION_PLAN.md` |
| **S0-T2** | ADRs: 001 Mother Agent as single orchestrator · 002 Domain model (`work/home/shared`) · 003 Permission modes & effect classes · 004 Content-free ledger | `docs/adr/*.md` |
| **S0-T3** | `Domain` enum; `domain` field on `CardRecord`, `AgentRecord`, `card_spawn` (default `shared`); serialize in `ui_sessions` JSON without migration | `src/state.rs`, `src/events/sse.rs`, `src/orchestrator/persist.rs` |
| **S0-T4** | Module skeletons with doc comments: `mother/`, `worlds/`, `intelligence/`, `memory/`, `permissions/`, `briefing/` | `src/lib.rs`, new `mod.rs` files |
| **S0-T5** | Agent contract types: `AgentSpec { id, world, mission, mode, memory_scope, tools: {name → effect} }`; extend `mcp_allowlists.toml` schema **backward-compatibly** (missing `mode` → `suggest`, missing effects → boot warning until S2 makes them required) | `src/tools/allowlist.rs`, `mcp_allowlists.toml` |
| **S0-T6** | Synthetic activity generator: persona config → N weeks of content-free events (JSONL), including a `drift` scenario (later work end, less reading) for S7/S8 tests | `scripts/synth_ledger.py` |
| **S0-T7** | CI: Postgres service container so migration and store tests run on every PR | `.github/workflows/ci.yml` |
| **S0-T8** | `RuntimeStatus.phase = "P2-S0"` in `/health`; ticket import for S1–S3 | `src/state.rs`, `src/main.rs` |

**Validation (product owner):**
- [ ] `cargo test` and CI green with Postgres service
- [ ] `/health` shows `milestone: "M11"` and `phase: "P2-S0"`
- [ ] `mcp_allowlists.toml` parses with and without `mode`/effects; boot logs a warning listing tools lacking effects
- [ ] `scripts/synth_ledger.py --weeks 6 --drift` produces JSONL with two visibly different regimes (spot-check)
- [ ] ADRs reviewed and accepted

**Dependencies:** none. **Risks:** scope creep into S1; keep S0 to plumbing only.

---

### S1 — Mother Agent v1

**Goal:** Replace "router + summarizer" with one orchestrator that classifies into worlds/agents, delegates, and synthesizes — while the seven existing scenarios keep working. Text, voice and AWP all reach the Mother Agent.

**User stories**
- As a user, "What's happening with work?" returns one answer composed from several agents.
- As a user, an ambiguous request gets one clarifying question, not a wrong scenario.
- As an external agent, `POST /awp/a2a` reaches the same orchestrator the UI uses.

| Task | Detail | Files |
|---|---|---|
| **S1-T1** | `intake`: `LlmConditionalAgent` → structured `IntakeResult { domains, targets[{world, agent, task}], kind: question\|action, urgency, clarify }`; keep `mock::pick_scenario` keyword fallback | `src/mother/intake.rs` (from `agents/router.rs`) |
| **S1-T2** | `mother_agent` `LlmAgent` with internal `MotherToolset`: `get_context`, `delegate`, `compose`. **No MCP tools.** System prompt encodes §3.1 responsibilities and §1.3 principles | `src/mother/agent.rs` |
| **S1-T3** | Delegation adapter: targets → existing runners (`deck`, `morning`, `live`, `people`, `week`, `lisbon`, `proactive`) so the tour stays live; parallel fan-out with per-target timeout; results as `card_*` SSE via `workflow.rs` | `src/mother/delegate.rs`, `src/orchestrator/dispatch.rs` |
| **S1-T4** | Synthesis: generalize `suzy::summarize` to N results; output = HTML summary + `actions[]` (mode field placeholder until S2) | `src/mother/synth.rs` (from `agents/suzy.rs`) |
| **S1-T5** | `POST /api/sessions/{sid}/chat` → SSE (reuses `suzy_summary`, `card_*`, `suggest`, `done`); multi-turn context = last K turns from session; capability `chat_mother` | `src/routes/chat.rs`, `business.toml` |
| **S1-T6** | Route `dispatch_intent` and `/awp/a2a` through the Mother Agent; keep `?demo=1` untouched | `src/orchestrator/dispatch.rs`, `src/routes/intent.rs` |
| **S1-T7** | Voice: `submit_intent` and `get_session_context` tools call the Mother Agent | `src/voice/realtime.rs` |
| **S1-T8** | Tests: the six §3.5 prompts route to expected targets (live with `GOOGLE_API_KEY`, keyword fallback without); A2A deck still yields three artifacts (`deck_workflow_writes_three_artifacts`) | `tests/validate.rs` |

**Validation:**
- [ ] "What's happening with work?" → ≥2 agents run (network tab shows their `card_*` events) → one Suzy summary
- [ ] "Build me a pitch deck" and the full tour still work unchanged
- [ ] "hmm" → one clarifying question with three suggestions
- [ ] `curl -X POST /awp/a2a -d '{"intent":"Start my day"}'` returns `scenario` and events
- [ ] Voice: "start my day" through `/ws/voice` triggers the same path

**Dependencies:** S0-T3, S0-T5. **Risks:** added latency (intake + synth) → flash-lite for both; cache intake per session for follow-ups; measure p95 before/after.

---

### S2 — Activity ledger + permission modes

**Goal:** Every meaningful event is recorded content-free, and every tool call passes a gate that enforces Observe / Suggest / Automate by effect class, producing pending actions and audit entries.

**User stories**
- As a user, I can set the Email agent to *Suggest* and see drafts appear without anything being sent.
- As a user, I approve or reject queued actions from a card or an inbox, singly or in batch.
- As a user, I can pause everything with one switch.

| Task | Detail | Files |
|---|---|---|
| **S2-T1** | `migrations/004_activity_events.sql` (schema §7.1); `LedgerService::record()` with batched async writes | `src/intelligence/ledger.rs` |
| **S2-T2** | Instrument: tool call/response in the workflow stream loop, card resolves, approvals/rejections, intent/chat turns; UI posts minimal focus/notification counts (`POST /api/sessions/{sid}/events`) | `src/orchestrator/workflow.rs`, `src/routes/events.rs`, `web/static/field-client.js` |
| **S2-T3** | Loader for S0-T6 JSONL → `activity_events` (dev/test only) | `src/bin/load_ledger.rs` or script |
| **S2-T4** | Effects required: every allowlisted tool gets an effect (`read`, `write_local`, `schedule_with_others`, `send_external`, `publish_public`, `financial`, `delete`); boot **fails** on a missing effect; classify all 14 servers | `mcp_allowlists.toml`, `src/tools/allowlist.rs` |
| **S2-T5** | `PermissionGate` toolset wrapper (pattern: `GeminiSanitizedToolset`/`FilteredToolset`): by mode × effect → pass / enqueue pending / deny; returns an explicit string to the LLM so it plans accordingly; agent prompt gets its mode | `src/permissions/gate.rs`, `src/agents/gemini.rs` |
| **S2-T6** | `migrations/005_permissions.sql`: `agent_permissions`, `pending_actions` (args encrypted), `audit_log` | — |
| **S2-T7** | Actions API: `GET /api/actions?status=pending`, `POST /api/actions/{id}/approve\|reject\|edit`, batch approve; execution via gate; audit row; SSE `permission_request`, `action_result`; expiry 48 h | `src/routes/actions.rs`, `src/events/sse.rs` |
| **S2-T8** | Permissions API: `GET/PUT /api/permissions` (per-agent mode, per-tool overrides); `POST /api/pause` (all / world / until); extends `AmbientStore` DND | `src/routes/permissions.rs`, `src/ambient/store.rs` |
| **S2-T9** | `POST /commit` becomes a thin wrapper that creates-and-approves a pending action (backward compatible with the UI) | `src/routes/commit.rs` |
| **S2-T10** | Tests: Suggest → `create_draft` executes, `send_draft` queued; Observe → `create_draft` denied with message; approve executes and audits; batch approve; pause blocks all writes; ledger has no content fields (schema test) | `tests/validate.rs` |

**Validation:**
- [ ] Set Email to *Suggest* → "Start my day" → "Draft replies" → drafts exist in Gmail drafts, nothing sent; approval inbox shows "Send 2 replies"
- [ ] Approve one → sent; `GET /api/audit` (raw for now) shows the row with effect `send_external`
- [ ] Set Email to *Observe* → the same flow yields a facts-only card and a "Suggest actions?" prompt
- [ ] `POST /api/pause` → any write returns "paused"
- [ ] `SELECT * FROM activity_events LIMIT 20` contains no subjects/bodies

**Dependencies:** S0-T5, S1-T3. **Risks:** LLM ignores "queued" tool result and retries → cap retries in gate; effect misclassification → review checklist per MCP server in the PR template.

---

### S3 — Personal memory v1

**Goal:** A memory store that separates known / assumed / recommended, scoped by domain, with provenance, encryption for sensitive values, and an API the user controls.

**User stories**
- As a user, "remember that I don't take meetings before 10" is stored as *known* and respected by Productivity.
- As a user, I can ask "why do you think that?" and see the evidence.
- As a user, I can delete a memory item or export everything.

| Task | Detail | Files |
|---|---|---|
| **S3-T1** | `migrations/006_memory.sql`: `memory_items` (§11.3), minimal `consents` | — |
| **S3-T2** | `MemoryService`: scoped `read`, `propose` (always `assumed`), `confirm`, `correct`, `forget`, `export`, `purge`; AES-GCM for `sensitivity ≥ sensitive` with per-user key wrapped by `MEMORY_MASTER_KEY` | `src/memory/service.rs`, `src/memory/crypto.rs`, `.env.example` |
| **S3-T3** | Tools: `read_memory(scope)`, `propose_memory`; Mother gets full scope; leaf agents get `AgentSpec.memory_scope` only | `src/memory/tools.rs`, `src/mother/agent.rs` |
| **S3-T4** | Chat: "remember …" → known item with `provenance.kind = user_statement`; "forget …"; "why do you think that?" → provenance chain | `src/mother/agent.rs` |
| **S3-T5** | Memory API: `GET /api/memory?kind&domain`, `PATCH /api/memory/{id}` (confirm/correct), `DELETE /api/memory/{id}`, `GET /api/memory/export`, `DELETE /api/memory` | `src/routes/memory.rs`, `business.toml` |
| **S3-T6** | Replace hardcoded facts with profile memory: `"San Francisco"` in `greeting/context.rs`, `country us` in brief prompts, default briefing time | `src/greeting/context.rs`, `src/agents/brief.rs` |
| **S3-T7** | Synthesis cites kind: "(you told me)" / "(I think, from 3 weeks of activity)" | `src/mother/synth.rs` |
| **S3-T8** | Tests: propose → assumed; confirm → known with appended provenance; home agent cannot read `work.*`; DB dump shows ciphertext for sensitive values; export round-trips | `tests/validate.rs` |

**Validation:**
- [ ] Say "Remember I never take meetings before 10" → `GET /api/memory` shows `known`, category `preference`
- [ ] Ask "Prepare me for my afternoon" → answer references the preference "(you told me)"
- [ ] Insert an assumed item via test → chat answer says "(I think)"; `PATCH` confirm → becomes known
- [ ] Set home location in memory → greeting weather uses it (no more hardcoded city)
- [ ] `DELETE /api/memory` → empty; export before delete produced JSON with provenance

**Dependencies:** S2 (audit for memory changes). **Risks:** key management — document rotation; never log values.

---

## 5. Release R2 — Two Worlds (S4–S6)

**Release goal:** two worlds, one briefing. Agents are organized by life domain under Work and Home mothers, they collaborate through a mediated bus, and the user gets a full daily briefing across both worlds.

**Release demo (end of S6):** 07:15 briefing arrives with Work / Home / Learning / Wellbeing / Balance / Suggested actions. An email asking for a Thursday meeting becomes one recommendation with a draft and prep; one approval sends and books. A family dinner overlapping a late review yields exactly one question.

---

### S4 — Work World

**Goal:** `work_mother` coordinates eight work agents: five extended from today's code, three new (two as labeled stubs where MCP servers do not exist yet).

**User stories**
- As a user, "What's happening with work?" gives me priorities, mail, team threads and project status in one answer.
- As a user, unanswered emails older than three days are flagged without my asking.
- As a user, my work tasks live in one list the agents can read and update (Suggest).

| Task | Detail | Files |
|---|---|---|
| **S4-T1** | `work_mother`: aggregates leaf results, enforces work scope, returns one structured result; parallel fan-out | `src/worlds/work.rs` |
| **S4-T2** | Re-home and rename: `productivity` (from `calendar_agent`), `email` (from `inbox_agent`), `team_comms` (from `team_agent` + `priya_agent` → generic 1:1 prep), `project` (from `focus_agent` + slack/crm), `research_knowledge` (from ambient `research_agent`), `work_automation` (wraps deck workflow) | `src/agents/work/*.rs`, `src/agents/{calendar,inbox,people,week}.rs` (thin re-exports during transition) |
| **S4-T3** | `migrations/007_tasks.sql`; Productivity tools `create_task`, `list_tasks`, `postpone_task` (increments `postponed_count`), `plan_day`; deadlines and focus sessions as tasks with kinds | `src/agents/work/productivity.rs`, `src/tools/tasks.rs` |
| **S4-T4** | Email: categorize; urgent detection; follow-up tracker (unanswered threads > N days from hashed subjects in ledger); scheduled send = pending action with `send_at` | `src/agents/work/email.rs` |
| **S4-T5** | New `career`: goals/skills/learning plan from memory; suggests next steps; job/network data as labeled stub (BK-101) | `src/agents/work/career.rs` |
| **S4-T6** | New `professional_social`: labeled stub for LinkedIn (BK-101); Observe via manual export import | `src/agents/work/professional_social.rs` |
| **S4-T7** | `mcp_allowlists.toml`: entries + effects + default modes for all work agents (§4 table) | `mcp_allowlists.toml` |
| **S4-T8** | Work People rail from `team_comms`; work-domain colour on cards | `src/rails/people.rs`, `web/index.html` |
| **S4-T9** | Tests: work fan-out ≥3 results; follow-up tracker flags a 3-day-old thread from synthetic data; finance/health tools absent from every work agent's toolset | `tests/validate.rs` |

**Validation:**
- [ ] "What's happening with work?" → cards for Productivity, Email, Team, Project; one synthesis
- [ ] "Add a task: finish board memo by Friday" → task created (Suggest mode → immediate, `write_local`)
- [ ] Follow-up card: "2 threads unanswered for 3+ days" (from real inbox or synthetic)
- [ ] Career and Professional Social cards show **labeled** stubs, not fake data
- [ ] Every work agent tile carries the blue domain colour

**Dependencies:** S1–S3. **Risks:** renaming churn breaks the demo tour → keep re-export shims one sprint.

---

### S5 — Home World

**Goal:** `home_mother` coordinates eight home agents with conservative defaults: finance is read-only, health never diagnoses, social accounts are Observe.

**User stories**
- As a user, "Remind me about family commitments" lists this week's family events and birthdays.
- As a user, my personal Google account is connected separately from work and never mixed.
- As a user, the Finance agent can only ever read.

| Task | Detail | Files |
|---|---|---|
| **S5-T1** | `home_mother` (mirror of S4-T1 with home scope) | `src/worlds/home.rs` |
| **S5-T2** | Re-home: `finance` (from `money_agent`), `health_wellness` (from `health_agent`), `entertainment` (from `headlines/now/markets` + `maker` playlists + Live rail), `social_fun` (from `maker`), `travel` (from `lisbon`) | `src/agents/home/*.rs` |
| **S5-T3** | New `family`: family calendar id (memory), `migrations/008_contacts.sql` + vCard/CSV import, important dates, household tasks (`tasks` with `domain = home`); Family rail populated | `src/agents/home/family.rs`, `scripts/import_contacts.py` |
| **S5-T4** | New `personal_productivity`: personal calendar, errands with `mcp-maps` routing, routines | `src/agents/home/personal_productivity.rs` |
| **S5-T5** | New `personal_social`: labeled stub (BK-102); Observe via export import | `src/agents/home/personal_social.rs` |
| **S5-T6** | Separate identities: OAuth connections carry `world = work\|home`; MCP calendar/email children spawned per identity; `.env.example` documents both | `src/auth.rs`, `src/main.rs`, `src/config.rs` |
| **S5-T7** | Guardrails: finance allowlist contains only `read` effects (boot check); health prompt + output lint (no diagnosis vocabulary; escalation sentence when thresholds cross, e.g. sleep < 5 h for 5 days → "consider speaking with a professional") | `mcp_allowlists.toml`, `src/agents/home/health_wellness.rs`, `src/intelligence/phrasing.rs` (lint stub) |
| **S5-T8** | HealthKit export importer (XML → CSV consumed by existing `HEALTH_CSV_PATH`) — BK-010 | `scripts/healthkit_import.py` |
| **S5-T9** | Tests: family routing; finance agent has zero non-read tools; health lint blocks a diagnosis sentence; identity separation (home agent cannot see work calendar id) | `tests/validate.rs` |

**Validation:**
- [ ] Connect a personal Google account → Family + Personal Productivity read it; work agents do not
- [ ] "Remind me about family commitments" → events + upcoming birthdays from imported contacts
- [ ] "How's my spending?" → Finance card, read-only; no "pay"/"transfer" buttons anywhere
- [ ] Health card with a low-sleep CSV shows facts + escalation sentence; no diagnosis
- [ ] Entertainment: "Play my morning brief" hands off to audio (Suggest → approve)

**Dependencies:** S4 patterns. **Risks:** two OAuth identities complicate `mcp-calendar` spawning → one child per identity, health-checked at boot.

---

### S6 — Cross-agent collaboration + Daily Briefing v2

**Goal:** Agents collaborate through a Mother-mediated bus with trace ids; the two flagship journeys run end-to-end; the full daily briefing is prepared on cron and readable by voice.

**User stories**
- As a user, a meeting request in email becomes one recommendation with a draft reply and prep — one approval does both.
- As a user, I get a morning briefing covering work, home, learning, wellbeing, balance and suggested actions.
- As a user, a work/home clash produces exactly one question.

| Task | Detail | Files |
|---|---|---|
| **S6-T1** | `AgentMessage` envelope (§14.1), broadcast bus (pattern: `AmbientStore`), trace ids, fan-out depth cap 2, timeouts | `src/mother/bus.rs` |
| **S6-T2** | World mothers publish/consume on the bus; Mother issues follow-up `request`s | `src/worlds/*.rs`, `src/mother/delegate.rs` |
| **S6-T3** | Journey §13.5 (email → availability → prep → one recommendation → approve sends + books) | agents `email`, `productivity`, `team_comms`, `project` |
| **S6-T4** | Journey §13.3 with a simple cross-calendar overlap check (full Balance Agent in S8) | `src/mother/arbitrate.rs` |
| **S6-T5** | Arbitration v1: protected time (from memory), dedupe conflicting proposals into one question, pass health/finance labels through untouched | `src/mother/arbitrate.rs` |
| **S6-T6** | `BriefingWorkflow`: Parallel[`work_mother`, `home_mother`, intelligence stub] → compose; sections Work / Home / Learning / Wellbeing / Balance / Suggested actions (with modes); honest empty states; SSE `briefing`; `GET /api/briefing/today`; `morning` scenario now runs the briefing | `src/briefing/*.rs`, `src/routes/briefing.rs`, `src/events/sse.rs` |
| **S6-T7** | Cron: prepare at `preference.briefing.time` (default 07:15) via `AmbientAgent`; respects quiet hours/DND | `src/briefing/schedule.rs` |
| **S6-T8** | Voice: "What do I need to know today?" reads the briefing; Entertainment hands off to audio if enabled | `src/voice/realtime.rs` |
| **S6-T9** | Tests: bus round-trip with trace id; briefing has six sections with empty states when integrations are missing; conflict fixture yields one question and two pending actions | `tests/validate.rs` |

**Validation:**
- [ ] Seed an email "Can we meet Thursday about the renewal?" → one card: proposed slot, draft, prep; **Approve** → reply sent, event created, two audit rows, one trace id
- [ ] Seed family dinner 18:30 + review 17:30–19:00 → briefing Balance line + one question; **Yes** → move proposal (pending) + attendee note (pending)
- [ ] 07:15 (or trigger) → briefing card with all sections; missing integrations show "not connected" not fake numbers
- [ ] Voice: "What do I need to know today?" → spoken briefing

**Dependencies:** S4, S5. **Risks:** briefing latency (many agents) → prepare on cron and cache; on-demand shows cached + "refreshing".

---

## 6. Release R3 — Intelligence (S7–S9)

**Release goal:** "it noticed my routine changed." The system learns a personal baseline, describes drift and imbalance in neutral language, detects work/home conflicts properly, and understands what the user reads.

**Release demo (end of S9):** With six weeks of (synthetic or real) activity, the briefing says: "Your routine has changed over the last two weeks… Would you like me to help you review what's changed?" — and "What have I been reading lately?" answers from the knowledge graph.

---

### S7 — Pattern Recognition + Personal Baseline

**Goal:** Deterministic pattern aggregation over the ledger; a rolling baseline per dimension with warm-up, robust statistics, drift detection, cooldown; observations phrased neutrally; a confirmation flow that turns learned baselines into known memory.

**User stories**
- As a user, nothing is flagged during my first 14 days, and I can see the warm-up progress.
- As a user, after a real change in my routine I get one neutral observation with an offer to review — not a lecture.
- As a user, I can say "this is my new normal" and the system re-learns.

| Task | Detail | Files |
|---|---|---|
| **S7-T1** | Patterns: SQL/Rust aggregations for §7.2 dimensions → `activity_daily (user_id, day, dimension, value)` | `src/intelligence/patterns.rs`, `migrations/009_baselines_observations.sql` |
| **S7-T2** | Baseline: median + MAD, weekday/weekend classes, 28-day window, 14-day warm-up, drift rule (§8.2), cooldown 7 d, "meaningful" rule (≥2 dims or 1 dim ≥10 d) | `src/intelligence/baseline.rs` |
| **S7-T3** | Phrasing: facts → neutral text via LLM constrained by §7.6 rules; **lint** rejects banned vocabulary and requires number + period + baseline reference; deterministic fallback template | `src/intelligence/phrasing.rs` |
| **S7-T4** | Jobs: nightly full recompute + 30-min incremental on `AmbientAgent`/`CronTrigger`; respect DND/quiet hours | `src/intelligence/jobs.rs`, `src/ambient/service.rs` |
| **S7-T5** | Observations API + SSE: `GET /api/observations`, `POST /api/observations/{id}/accept\|dismiss\|snooze\|correct`; `observation` event | `src/routes/observations.rs`, `src/events/sse.rs` |
| **S7-T6** | Mother: `read_observations` tool; surfaces only in briefing or on "how am I doing?"; never mid-task | `src/mother/agent.rs`, `src/briefing/` |
| **S7-T7** | Confirmation: after warm-up, "Is this your usual routine?" card → `baselines.confirmed = true` + known memory `routine.*`; "re-learn" action resets window | `src/intelligence/baseline.rs`, `src/memory/` |
| **S7-T8** | Dimension toggles in preferences (user can switch any dimension off) | `src/routes/permissions.rs` or memory prefs |
| **S7-T9** | Tests with synthetic data: 4 weeks normal + 2 weeks drift → exactly one `drift` observation, text passes lint, contains "two weeks"; warm-up suppresses; cooldown suppresses repeats; disabled dimension ignored | `tests/validate.rs` |

**Validation:**
- [ ] Load `synth_ledger.py --weeks 6 --drift` → run nightly job → `GET /api/observations` shows one drift observation with the §8.1 wording shape
- [ ] Briefing includes it under Wellbeing/Balance with the offer; **Snooze** → not shown for 7 days
- [ ] Load 10 days only → no observations; UI shows "learning your routine — 10 of 14 days"
- [ ] "Is this your usual routine?" → **Yes** → `routine.work.end` appears as *known* in memory
- [ ] Turn off the `social.minutes` dimension → job skips it

**Dependencies:** S2 ledger, S3 memory, S6 briefing. **Risks:** real users have irregular weeks → MAD floors per dimension; log false positives from user dismissals to tune k.

---

### S8 — Balance Agent + Digital Behavior Agent

**Goal:** Work vs Home attention accounting, boundaries the user declares, proper cross-calendar conflict detection, and neutral digital-behaviour observations.

**User stories**
- As a user, I can declare working hours, protected time, quiet hours and focus blocks, and the agents respect them.
- As a user, I'm told when this week is heavier on work than my usual, in numbers, with an offer.
- As a user, a work meeting drifting into a family commitment produces one question before it happens.

| Task | Detail | Files |
|---|---|---|
| **S8-T1** | Balance: attention share (day/week), spillover after `work.end`, weekend work, postponement debt (tasks with `postponed_count ≥ 3`), family/social cadence vs baseline → `balance` observations | `src/intelligence/balance.rs` |
| **S8-T2** | Conflict detection across both identities' calendars + protected time → `conflict` observations; replaces S6-T4 check | `src/intelligence/balance.rs`, `src/mother/arbitrate.rs` |
| **S8-T3** | Boundaries: `migrations/…boundaries` (or memory category); `GET/PUT /api/boundaries`; enforcement — work agents drop to Observe outside working hours (queue attention for morning), Mother refuses to schedule into protected time without asking, quiet hours suppress attention cards/voice | `src/routes/boundaries.rs`, `src/permissions/gate.rs`, `src/mother/arbitrate.rs` |
| **S8-T4** | Digital Behavior: context switching (domain/agent switches per hour), notification load, long uninterrupted work (>120 min), distractions in focus blocks, social consumption vs baseline; thresholds in preferences | `src/intelligence/behavior.rs` |
| **S8-T5** | UI hooks (content-free): focus/blur, card opens per domain, notification counts → `POST /api/sessions/{sid}/events` | `web/static/field-client.js` |
| **S8-T6** | Briefing Balance section from real observations ("relatively work-heavy today: 7.5 h meetings, one personal commitment at 18:30") | `src/briefing/` |
| **S8-T7** | Tests: seeded spillover → one balance observation; seeded conflict → one question + two pending actions; lint blocks "too much"/"should"; outside working hours the email agent's `create_draft` is queued, not run | `tests/validate.rs` |

**Validation:**
- [ ] Set working hours 09:00–17:30, protected 18:00–21:00 → at 18:30 "draft replies" queues for morning with a note
- [ ] Synthetic heavy week → briefing: "about 11 hours more than your 4-week average" + offer
- [ ] Family dinner + late review fixture → exactly one question; **Yes** → proposals (pending)
- [ ] Behaviour observation after a seeded 3-hour block: "two 3-hour blocks without a break yesterday" (neutral)

**Dependencies:** S7. **Risks:** over-notification → all observations flow through briefing/cooldown, never push.

---

### S9 — Reading & Knowledge Agent

**Goal:** A reading ledger and a personal knowledge graph that answer "what have I been reading?", feed the briefing's Learning section, and inform the Career agent.

**User stories**
- As a user, "What have I been reading lately?" summarizes topics, frequency and trend, and links them to my projects.
- As a user, my briefing has a Learning section with course progress and reading trend.

| Task | Detail | Files |
|---|---|---|
| **S9-T1** | Reading events: Entertainment/news opens, research briefs, artifact opens, imports (browser reading list JSON, Kindle highlights CSV) → ledger `reading` (content-free) + `reading_items` (titles/topics, consent category `reading`) | `src/intelligence/knowledge.rs`, `scripts/import_reading.py` |
| **S9-T2** | `migrations/010_knowledge.sql`: `reading_items`, `knowledge_nodes`, `knowledge_edges` | — |
| **S9-T3** | Topic extraction nightly (LLM, batched), weights = recency × frequency; edges to projects (Project agent) and goals (Career/memory) | `src/intelligence/knowledge.rs`, `src/intelligence/jobs.rs` |
| **S9-T4** | `reading_knowledge` agent + Mother tool `query_knowledge`; answers §13.6 | `src/agents/shared/reading_knowledge.rs`, `src/mother/agent.rs` |
| **S9-T5** | Briefing Learning section; Career agent consumes rising topics; Research agent schedules briefs on rising topics (Observe) | `src/briefing/`, `src/agents/work/{career,research_knowledge}.rs` |
| **S9-T6** | Knowledge view (list of topics with trend, related items/projects) | `web/static/knowledge.js` |
| **S9-T7** | Tests: imported items → topics → trend query; consent revoked → import refused; graph export | `tests/validate.rs` |

**Validation:**
- [ ] Import a reading list → "What have I been reading lately?" → topics with counts and trend; "connects to project X"
- [ ] Briefing Learning: course progress (from tasks) + reading trend
- [ ] Revoke `reading` consent → import refused and existing items purge on the retention job (S11 completes purge)

**Dependencies:** S7 jobs, S3 consents. **Risks:** topic extraction quality → keep taxonomy small (≤ 50 topics per user), user can rename/merge.

---

## 7. Release R4 — Command Center & Trust (S10–S12)

**Release goal:** public beta. The user sees one command center (TODAY, Mother chat, approvals), controls every agent's authority and every memory item, and can grant narrow automations that run audited while they live.

**Release demo (beta gate, end of S12):** the full §13 journey set on a real account, desktop and mobile Safari, plus an external agent reaching the Mother Agent over AWP and receiving Suggest-mode outcomes.

---

### S10 — Command center UI: TODAY + Mother chat

**Goal:** A standing TODAY view composed by the server, a Work/Home lens, a persistent Mother chat, and an approvals inbox — alongside the existing spatial field.

**User stories**
- As a user, I open the app and see today across work and home without asking.
- As a user, I can switch to a Work-only or Home-only lens.
- As a user, Suzy is always one line away, by text or voice.

| Task | Detail | Files |
|---|---|---|
| **S10-T1** | `GET /api/today`: server-composed payload — work priorities, personal priorities, calendar (both), important messages, wellness reminders, family events, reading, entertainment, suggested actions with modes, open observations | `src/routes/today.rs`, `src/briefing/today.rs` |
| **S10-T2** | TODAY layout (Appendix E): panels, domain colours, mode badges on agent tiles, lens toggle (Both / Work / Home) persisted in preferences | `web/static/today.js`, `web/index.html` |
| **S10-T3** | Persistent Mother chat panel using `/chat` SSE; multi-turn; voice button reuses `live-voice.js` | `web/static/mother-chat.js` |
| **S10-T4** | Approvals inbox + observation feed: approve/edit/reject, batch, undo where available; keyboard accessible | `web/static/approvals.js` |
| **S10-T5** | Mobile ≤ 900 px layouts for TODAY / chat / approvals; intent input ≥ 16 px (NFR-009) | `web/index.html` CSS |
| **S10-T6** | Demo mode: `?demo=1` renders a scripted TODAY fixture, labeled "simulated" (NFR-008) | `web/static/today.js` |
| **S10-T7** | Playwright smoke: load TODAY, approve one action, switch lens (extends BK-001) | `scripts/smoke-mobile.mjs` |

**Validation:**
- [ ] Sign in → TODAY populated from real integrations; missing ones show "connect" chips, not fake data
- [ ] Lens = Work hides all home panels and green tiles; Home hides work
- [ ] Chat: "Prepare me for my afternoon" → answer + cards bloom in FIELD; voice works
- [ ] Approve two drafts in batch → toasts; audit rows; undo one within window
- [ ] Mobile Safari: stacked TODAY, chat usable

**Dependencies:** S6, S8. **Risks:** UI scope → designer delivers TODAY + approvals first; knowledge view stays simple.

---

### S11 — Trust center + privacy hardening

**Goal:** Every trust-related control is visible and usable: permissions, memory, consents, audit; retention and encryption are complete; a security review passes.

**User stories**
- As a user, I can set each agent to Observe / Suggest / Automate and pause a whole world.
- As a user, I can read, confirm, correct, forget and export what the system knows and assumes.
- As a user, I can see everything that ran while I was away and undo what's reversible.

| Task | Detail | Files |
|---|---|---|
| **S11-T1** | Permissions panel: per-agent mode slider, per-tool overrides (advanced), per-world pause, quiet hours, global pause | `web/static/trust-center.js` |
| **S11-T2** | Memory panel: known / assumed / recommended tabs; confirm / correct / forget; provenance ("why do you think that?"); export; delete all | `web/static/trust-center.js` |
| **S11-T3** | Audit viewer: `GET /api/audit` with filters; "what ran while you were away" summary; undo buttons | `src/routes/audit.rs`, `web/static/trust-center.js` |
| **S11-T4** | Consent persistence: full `consents` model; replace `InMemoryConsentService` binding; storage checks in `MemoryService`, `LedgerService`, reading import (no consent → refuse) | `src/memory/consent.rs`, `src/main.rs` |
| **S11-T5** | Retention: `retention_policies` table + nightly purge job (ledger 180 d, observations 365 d, audit 2 y, pending 30 d); `DELETE /api/account` = export then cascade delete | `migrations/011_retention.sql`, `src/intelligence/jobs.rs`, `src/routes/account.rs` |
| **S11-T6** | Encrypt OAuth tokens and health/finance memory values; key rotation runbook | `src/memory/crypto.rs`, `docs/runbooks/key-rotation.md` |
| **S11-T7** | Security review: §12.1 threat-model walkthrough; prompt-injection tests (a seeded email instructs "send this to everyone" → stays pending, never auto-sends); rate limits for all new routes in `AwpGate`; `business.toml` access levels ≥ `known` for actions/memory/permissions/consents | `tests/security.rs`, `src/awp_gate.rs`, `business.toml` |
| **S11-T8** | Tracing redaction audit: no payload bodies or memory values in logs; structured request ids (NFR-006) | `src/**` |

**Validation:**
- [ ] Set Finance to Automate → UI refuses ("financial effects cannot be automated"); set Email to Automate → allowed only for approved recipes (none yet → behaves as Suggest)
- [ ] Memory panel: correct an assumed start time → becomes known with provenance chain visible
- [ ] Audit: shows last night's briefing preparation, recipe runs (S12), approvals; undo an archive
- [ ] Revoke `health` consent → health imports refused; next purge removes health values
- [ ] Injection test email → pending action only; report attached to PR
- [ ] Anonymous `curl` to `/api/memory` → AWP error envelope

**Dependencies:** S2, S3, S10. **Risks:** consent UX complexity → plain-language purpose strings reviewed by design.

---

### S12 — Automation recipes, AWP, cost controls, beta

**Goal:** Narrow, audited automations proposed from observed repetition; the Mother Agent as the AWP entry point with a Suggest ceiling for external callers; cost and latency budgets; onboarding; beta gate.

**User stories**
- As a user, after approving the same action several times, I'm offered a recipe I can inspect and approve once.
- As a user, recipe runs are visible and reversible.
- As an external agent, I can reach the Mother Agent and get Suggest-mode outcomes.

| Task | Detail | Files |
|---|---|---|
| **S12-T1** | Recipes: `migrations/012_recipes.sql`; model (trigger, condition evaluated **in code**, action, effect ceiling, undo window); `GET/POST /api/recipes`, approve/pause | `src/permissions/recipes.rs`, `src/routes/recipes.rs` |
| **S12-T2** | Work Automation: repetition detector (recurring `subject_hash`, approval streaks ≥ N) → recipe proposals as `recommended` memory + pending approval with exact condition/effect shown | `src/agents/work/work_automation.rs`, `src/intelligence/patterns.rs` |
| **S12-T3** | Recipe runner on `AmbientAgent` cron; gate honours recipe in Automate mode; audit with undo tokens; `publish_public`/`financial` never automatable (hard rule in gate) | `src/permissions/gate.rs`, `src/permissions/runner.rs` |
| **S12-T4** | Home recipes: recurring household tasks, weekly shopping list (Family), morning audio start (Entertainment), medication/appointment reminders (Health, reminders only) | `src/agents/home/*.rs` |
| **S12-T5** | AWP: `business.toml` capabilities for chat/today/briefing/actions/memory/permissions/recipes with access levels; typed A2A messages → Mother; external callers capped at Suggest regardless of user modes; opt-in `observation.created` event subscription | `business.toml`, `src/routes/intent.rs`, `src/awp_gate.rs` |
| **S12-T6** | Cost & latency: per-turn token budget; model routing (flash-lite for intake/synth/phrasing, stronger model for delegation planning only); intake cache; SLO check — first card < 3 s p95 (NFR-001) still holds with the Mother in the loop | `src/mother/*.rs`, `src/config.rs` |
| **S12-T7** | Onboarding (§13.8): connect worlds, consents with purposes, default modes (all Observe; suggest a few), 14-day warm-up notice | `web/static/onboarding.js`, `src/routes/onboarding.rs` |
| **S12-T8** | Regression suite + docs: extend Phase 1 suite with §13 journeys; `docs/IMPLEMENTATION_PLAN.md` Phase 2 table; deploy via existing `deploy/` | `docs/`, `.github/workflows/ci.yml` |
| **S12-T9** | Beta gate script (below) | — |

**Validation (beta gate):**
- [ ] Approve "archive newsletter" 9× → recipe proposed with condition + effect + undo window; approve → ⚡ badge on Email for that recipe only; runs appear in audit; undo works
- [ ] Attempt to create a recipe with `financial` effect → refused
- [ ] External agent: `POST /awp/a2a` "Draft a reply to the client contract email" → pending action, never sent
- [ ] Onboarding on a fresh account → next morning's briefing arrives at the chosen time
- [ ] Full §13.1–13.8 journeys pass on desktop + mobile Safari; regression suite green; p95 first card < 3 s

**Dependencies:** S2, S11. **Risks:** recipe conditions expressed loosely → conditions are a small typed DSL evaluated in Rust, never free text to the LLM.

---

## 8. Cross-cutting backlog (Phase 2)

| ID | Item | Needed by | Notes |
|---|---|---|---|
| **BK-101** | LinkedIn / professional-network MCP (mentions, posts, connections) | S4 (stub), post-beta (real) | Until then: labeled stub + manual export import |
| **BK-102** | Personal social MCPs (Instagram, Facebook, TikTok, X, WhatsApp) | S5 (stub), post-beta | Observe-only first; `publish_public` never automatable |
| **BK-103** | Contacts MCP (Google/Apple contacts) replacing vCard import | S5 | — |
| **BK-104** | Media MCP (music/podcast providers) for AI Radio | S5 (optional), S12 | `media-mcp` referenced in Phase 1 spec |
| **BK-105** | Tasks provider sync (Google Tasks, Todoist, Reminders) for the `tasks` store | S4 | Local store first |
| **BK-106** | Teams / Discord / WhatsApp Business adapters for Team Communication | post-beta | Same agent, new toolsets |
| **BK-107** | Wearable/HealthKit live MCP (carries Phase 1 BK-010) | S5 import path; live post-beta | — |
| **BK-108** | Travel MCP (carries Phase 1 BK-009) | post-beta | Flights/stay remain labeled stubs |
| **BK-109** | Local-first mode (ledger/memory/baseline on device) | post-beta | Requires content-free ledger (S2) — already designed for it |
| **BK-110** | SSE contract types shared with the front end (carries BK-002/BK-003) | S1+ | Generate TS types from `FieldEvent` |
| **BK-111** | Playwright visual regression for TODAY / trust center (carries BK-001) | S10 | — |
| **BK-112** | Key management: KMS-backed master key, rotation automation | S11 runbook; automation post-beta | — |
| **BK-113** | Multi-language phrasing for observations and briefing | post-beta | `business.toml` languages |
| **BK-114** | Household mode (multi-person) | post-beta | §15 |

---

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Mother Agent adds latency to every turn | UX regresses vs today's direct routing | Small model for intake/synth; intake cache; parallel delegation; measure p95 in S1 and S12 |
| Effect misclassification lets a write slip through | Trust failure | Boot fails on missing effect; PR checklist per MCP server; injection tests in S11; conservative defaults (unknown → most restrictive) |
| Prompt injection through email/Slack content | Unwanted action | Effects above `write_local` always require approval unless a **coded** recipe condition matches; recipes never automate `publish_public`/`financial` |
| Baseline false positives annoy users | Feature turned off | 14-day warm-up, MAD floors, meaningful rule, cooldown, dismiss feedback loop, per-dimension toggles |
| Perceived surveillance | Adoption | Content-free ledger, visible dimensions, deletable data, observations always show their facts, never push |
| OAuth for two identities + banking + health | Delays S5 | Existing MCP OAuth patterns; fixture accounts in CI; health via CSV import path first |
| MCP gaps (LinkedIn, social, contacts, media, travel) | Home/Work agents look thin | Labeled stubs (no fake data) + import paths; BK-101…108 tracked; roadmap honesty in UI |
| Cost per active user (many agents per turn) | Unit economics | Token budgets, model routing, briefing on cron + cache, deterministic intelligence layer |
| Team capacity smaller than assumed | Schedule | Releases are cut-lines: R1+R2 alone is a coherent product; R3 and R4 can stretch |
| Renaming/re-homing breaks the Phase 1 demo tour | Marketing captures fail | Shim re-exports for one sprint; regression suite from Phase 1 runs every PR |

---

## 10. Metrics to track from R1 onward

| Metric | Why | Target at beta |
|---|---|---|
| p95 chat turn latency; p95 first card | Orchestration overhead | first card < 3 s; chat turn < 6 s |
| Turns answered by ≥ 2 agents | Is it an OS or a chatbot | > 40 % of turns |
| Pending-action approval / edit / rejection rates | Suggestion quality | approval > 60 %, rejection < 15 % |
| Automate runs per active user per week; undo rate | Trust earned | undo < 5 % |
| Drift observations accepted vs dismissed; "false" flags | Baseline quality | dismissed-as-wrong < 20 % |
| Assumed memory items corrected by users | Inference quality | corrected < 25 % |
| Cross-domain access attempts blocked by the gate (should be zero in prod) | Isolation | 0 |
| Briefing open rate; time from onboarding to first briefing | Core loop | > 70 %; next morning |
| LLM cost per active user per day | Economics | budget set in S12 |

---

## 11. Phase 2 definition of done (beta)

- [ ] S0–S12 merged and tagged; `docs/IMPLEMENTATION_PLAN.md` Phase 2 table complete
- [ ] Every agent in the catalog (§6.2) exists as real agent or **labeled** stub — no silent fake data
- [ ] Every write effect passes the permission gate; audit coverage test = 100 % of `send_external`, `schedule_with_others`, `delete`, `financial`, `publish_public`
- [ ] Memory shows kind and provenance for every item; export and delete work
- [ ] Baseline warm-up, drift, balance and behaviour observations pass the neutral-language lint
- [ ] Consents persisted and enforced; retention purge running; sensitive columns encrypted
- [ ] TODAY, Mother chat, approvals, trust center usable on desktop and mobile Safari
- [ ] AWP manifest lists all Phase 2 capabilities with access levels; external callers capped at Suggest
- [ ] Regression suite (Phase 1 + §13 journeys) green on the production URL
- [ ] Security review (S11-T7) signed off

---

## 12. After beta — candidate themes

Household mode · device layer (watch/car/speaker) · local-first · agent marketplace via AWP · employer-managed Work World · life-event modes · richer knowledge graph · proactive AI radio · open ledger/memory export standard. See `PERSONAL_AI_OS.md` §15.

---

*Update the Progress table in §2 as sprints merge.*
