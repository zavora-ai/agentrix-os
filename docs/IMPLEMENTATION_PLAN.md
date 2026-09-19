# Agentrix OS — Implementation Plan

Tick boxes as you complete work. Merge to `main` only after the milestone **validation** section is fully checked.

**Spec:** [`SPECIFICATION.md`](./SPECIFICATION.md) (v1.2)  
**Started:** 2026-06-22

---

## Progress summary

| Milestone | Branch | Status | Tag |
|-----------|--------|--------|-----|
| M0 Live shell | `milestone/M0-live-shell` | ✅ done | `v0.0.1-m0` |
| M1 Real deck | `milestone/M1-deck` | ✅ done | `v0.1.0-m1` |
| M2 Combine | `milestone/M2-combine` | ✅ done | `v0.2.0-m2` |
| M3 Morning | `milestone/M3-morning` | ✅ done | `v0.3.0-m3` |
| M4 Persistence | `milestone/M4-persistence` | ✅ code on `main` (validation gate unrecorded) | `v0.4.0-m4` |
| M5 Suzy + router | `milestone/M5-coordinator` | ✅ code on `main` — `src/agents/{suzy,router}.rs` (validation gate unrecorded) | `v0.5.0-m5` |
| M6 Scenarios | `milestone/M6-scenarios` | ✅ code on `main` — live/people/week/lisbon workflows; flights/health/stay are labeled stubs (BK-009/010) | `v0.6.0-m6` |
| M7 Proactive | `milestone/M7-proactive` | ✅ code on `main` — `src/ambient/`, AWP event subscriptions | `v0.7.0-m7` |
| M8 Rails polish | `milestone/M8-rails` | ✅ code on `main` — `src/rails/`, `/api/people`, `/api/live`, `/api/rails/background` | `v0.8.0-m8` |
| M9 Auth + DB | `milestone/M9-auth` | ✅ code on `main` — `pg_session.rs`, `auth.rs`, migrations 001–003, allowlist catalog | `v0.9.0-m9` |
| M10 Live voice | `milestone/M10-voice` | ✅ code on `main` — `src/voice/`, `/ws/voice` | `v1.0.0-m10` |
| M11 Deploy | `milestone/M11-deploy` | ✅ code on `main` — Dockerfile, Caddy, CI; runtime reports `milestone: "M11"` | `v1.0.0` |

> **Reconciled 2026-09-19 (Phase 2 · S0-T1).** Statuses above were derived from the code and CI on `main` at
> `228ec78`, not from recorded sign-offs: the per-milestone **Validation** checklists for M0–M11 were never
> ticked in this file. Upstream has no release tags yet, so the Tag column lists the *intended* tags.
> Treat the unticked validation scripts as an open ticket (BK-100) rather than as missing code.

## Phase 2 — Personal AI OS (Release R1 · Foundation)

Concept: [`PERSONAL_AI_OS.md`](./PERSONAL_AI_OS.md) · Plan: [`SPRINT_PLAN.md`](./SPRINT_PLAN.md) · ADRs: [`adr/`](./adr/)

| Sprint | Branch | Status | Tag |
|--------|--------|--------|-----|
| S0 Align & scaffold | `phase2/r1-foundation` | ✅ code + tests (validation script below awaits product-owner sign-off) | `v1.0.1-p2` |
| S1 Mother Agent v1 | `phase2/r1-foundation` | ✅ code + tests (validation awaits sign-off) | `v1.1.0-p2` |
| S2 Ledger + permission modes | `phase2/r1-foundation` | ✅ code + tests (validation awaits sign-off) | `v1.2.0-p2` |
| S3 Personal memory v1 | `phase2/r1-foundation` | ✅ code + tests (validation awaits sign-off) | `v1.3.0-p2` |

### R1 validation script (product owner)

Offline (no API key, no Postgres) — everything streams its mock, the Mother still orchestrates:

- [ ] `cargo test --lib` and `cargo test --test validate -- mother_ domain_ gate_ permission_ pending_ ledger_ memory_ effects_` are green
- [ ] `cargo run` → `GET /health` reports `"phase": "P2-S3"`; `GET /awp/manifest` lists `chat_mother`, `list_actions`, `approve_action`, `get_permissions`, `set_permissions`, `pause_agents`, `record_ui_events`, `manage_memory`
- [ ] `POST /api/sessions/{sid}/chat {"text":"What's happening with work?"}` → one `scenario` event with `total_cards: 6`, cards re-indexed 0–5, one `suzy_summary` with `key: "mother"`, `done`
- [ ] `{"text":"Build me a pitch deck"}` → unchanged deck stream; `{"text":"hmm"}` → one clarifying question
- [ ] `{"text":"Remember I live in Nairobi"}` → "I'll remember" reply; `GET /api/greeting?session_id={sid}` uses Nairobi for weather when the weather MCP is connected
- [ ] `GET /api/memory?session_id={sid}` without a JWT → AWP error envelope (403); with dev sign-in → the item with `kind: "known"` and `provenance[0].kind: "user_statement"`

Live (API key + `mcp-email` connected):

- [ ] `PUT /api/permissions {"agent_id":"inbox_agent","mode":"suggest"}` → "Start my day" → "Draft replies" creates drafts (write_local runs), nothing is sent
- [ ] Any `send_*` tool call → `permission_request` event on the stream and a row in `GET /api/actions`; `POST /api/actions/{id}/approve` executes it and `GET /api/audit` shows `decision: "approved"`
- [ ] `PUT /api/permissions {"agent_id":"inbox_agent","mode":"observe"}` → the same flow yields facts only and `GET /api/audit` shows `denied`
- [ ] `POST /api/pause {"scope":"all"}` → every write returns `status: "paused"`; `POST /api/resume` restores
- [ ] `SELECT meta, subject_hash FROM activity_events LIMIT 20` contains no bodies, subjects or names

**Legend:** ⬜ not started · 🟡 in progress · ✅ done

---

## How to work each milestone

1. Create branch `milestone/Mx-…`
2. Complete **Tasks** checkboxes
3. Run `cargo check`, `cargo test`, `cargo clippy`
4. Record screen capture of **Validation** script
5. User signs off → merge `main` → tag `v0.Mx.0`
6. Update **Progress summary** above

---

## M0 — Live shell

**Goal:** Server drives UI via SSE; AWP discovery live; demo mode preserved.

### Tasks

- [x] **M0-T1** `src/main.rs` — Axum serves `web/index.html`, `audio/`, `static/`
- [x] **M0-T2** `src/config.rs` — `PORT`, paths, `.env` loading
- [x] **M0-T3** `src/state.rs` — `AppState` with in-memory session service
- [x] **M0-T4** `POST /api/sessions` → `{ session_id, user_id }`
- [x] **M0-T5** `POST /api/sessions/{sid}/intent` → SSE stream (mock deck events from server)
- [x] **M0-T6** `src/events/sse.rs` — event types: `scenario`, `card_spawn`, `card_status`, `card_resolve`, `suzy_summary`, `done`
- [x] **M0-T7** Extract `static/field-client.js` — SSE consumer wired to `buildCard` / `resolve` / `showSuzy`
- [x] **M0-T8** `web/index.html` — script include + `?demo=1` / `localStorage` flag keeps legacy `scenarios{}` path
- [x] **M0-T9** `business.toml` — site identity, brand_voice, `submit_intent` capability
- [x] **M0-T10** `adk-awp` path deps in `Cargo.toml`; `awp_routes()` merged in `main.rs`
- [x] **M0-T11** `POST /awp/a2a` stub → routes to same intent handler (BK-012)
- [x] **M0-T12** `GET /health` endpoint

### Validation (user gate)

- [ ] `cargo run` → `http://localhost:8080` loads greeting + field
- [ ] "Build me a pitch deck" — cards bloom from SSE (Network tab), not `setInterval`
- [ ] `?demo=1` — offline simulation still works
- [ ] Greeting + early-access signup unchanged
- [ ] `curl localhost:8080/.well-known/awp.json` — valid JSON
- [ ] `curl localhost:8080/awp/manifest` — lists `submit_intent`

### Commits

- [x] `chore(structure): organize project layout for M0`
- [x] `feat(m0): axum server with SSE intent stream and AWP discovery`

---

## M1 — Real deck workflow

**Goal:** `.xlsx`, `.docx`, `.pptx` artifacts via MCP; slides waits for excel + docs.

**MCP Phase A:** worksheet-mcp · docx-mcp · slides-mcp-server

### Tasks

- [x] **M1-T1** `src/tools/mcp.rs` — spawn 3 MCP children at boot; health check
- [x] **M1-T2** Env vars: `MCP_WORKSHEET_PATH`, `MCP_DOCX_PATH`, `MCP_SLIDES_PATH`
- [x] **M1-T3** `excel_agent` — worksheet-mcp toolset + system prompt
- [x] **M1-T4** `docs_agent` — docx-mcp toolset + system prompt
- [x] **M1-T5** `slides_agent` — slides-mcp-server toolset; reads sibling artifact paths from session state
- [x] **M1-T6** `DeckWorkflow` — `ParallelAgent[excel, docs]` → `SequentialAgent[slides]`
- [x] **M1-T7** Wire `adk-runner` → SSE (`card_status`, `card_resolve`, `card_surface`)
- [x] **M1-T8** Artifact dir `artifacts/{session_id}/`; `GET /artifacts/{path}`
- [x] **M1-T9** Slides filmstrip — `card_surface` events from `add_slide` / `describe_presentation`
- [x] **M1-T10** Tool allowlist per sub-agent (BK-005)
- [x] **M1-T11** Update `business.toml` — `list_artifacts`, deck capabilities
- [x] **M1-T12** Integration test: deck intent → three files exist (BK-004)

### Validation

- [ ] "Build me a pitch deck" → downloadable `.xlsx`, `.docx`, `.pptx`
- [ ] Auto-Slides shows `waiting…` until excel + docs done
- [ ] Filmstrip increments on real `add_slide` tool calls
- [ ] Open/Save returns real artifact URLs
- [ ] Refresh loses cards (expected until M4)

### Commits

- [x] `feat(mcp): spawn worksheet, docx, and slides servers`
- [x] `feat(agents): deck workflow with slides_agent`
- [x] `feat(ui): auto-surface preview from SSE`

---

## M2 — Action turn: combine

**Goal:** "combine" merges real deck; Conducting synced to server.

### Tasks

- [x] **M2-T1** `POST /api/sessions/{sid}/action` — action verb detection
- [x] **M2-T2** `combine_agent` — merge excel + docs into `.pptx` via slides-mcp
- [x] **M2-T3** SSE `conduct` events — step sequence for hand animation
- [x] **M2-T4** `field-client.js` — consume `conduct` events in `conductAction()`
- [x] **M2-T5** `finishDeck()` — pinned card + real artifact + `deck_done` audio
- [x] **M2-T6** `POST /api/sessions/{sid}/fuse` — generic card fuse
- [x] **M2-T7** `business.toml` — `submit_action`, `fuse_cards` capabilities

### Validation

- [ ] Deck scenario → "combine" → pinned Auto-Slides card
- [ ] Final `.pptx` downloadable; slide count matches `describe_presentation`
- [ ] Excel data in tables/charts; docx narrative in titles/bullets
- [ ] `POST /awp/a2a` with combine intent → same artifact as browser

### Commits

- [x] `feat(api): action turn + fuse endpoints`
- [x] `feat(agents): combine_agent via slides-mcp`
- [x] `feat(ui): server-driven conducting`

---

## M3 — Morning scenario

**Goal:** Real calendar, inbox, brief via MCP.

**MCP Phase B:** mcp-calendar · mcp-email · mcp-news · mcp-weather

### Tasks

- [x] **M3-T1** Boot Phase B MCP servers
- [x] **M3-T2** `MorningWorkflow` — parallel calendar + inbox → sequential brief
- [x] **M3-T3** `calendar_agent` — `get_today`, `list_events`, `find_free_time`
- [x] **M3-T4** `inbox_agent` — `list_inbox`, `search_emails`, `create_draft`
- [x] **M3-T5** `brief_agent` — `gnews_top_headlines` + `get_forecast`
- [x] **M3-T6** `GET /api/greeting` — personalized text from calendar
- [x] **M3-T7** OAuth flow for Google (calendar + email)
- [x] **M3-T8** `business.toml` — `get_greeting` capability

### Validation

- [ ] Connect test Google account
- [ ] "Start my day" → real meeting count + real flagged emails
- [ ] Brief has live headline + weather for location
- [ ] Greeting reflects actual calendar

### Commits

- [x] `feat(mcp): phase B calendar email news weather`
- [x] `feat(agents): morning workflow`
- [x] `feat(api): personalized greeting`

---

## M4 — VerbUI persistence

**Goal:** Snooze/wake/commit survive refresh; commits execute real actions.

### Tasks

- [x] **M4-T1** Session state: `app:cards`, `app:agents:active`, `app:agents:resting`
- [x] **M4-T2** `GET /api/sessions/{sid}/cards` — hydrate field on load
- [x] **M4-T3** `GET /api/sessions/{sid}/agents` — hydrate rails
- [x] **M4-T4** `POST /api/agents/{id}/snooze`
- [x] **M4-T5** `POST /api/agents/{id}/wake`
- [x] **M4-T6** `POST /api/sessions/{sid}/commit` — label → tool call (e.g. `send_draft`)
- [x] **M4-T7** `field-client.js` — call snooze/wake/commit APIs instead of local-only

### Validation

- [ ] Fling card → refresh → still in Resting rail
- [ ] Wake → back to Active
- [ ] Inbox "Draft replies" commit → real draft created

### Commits

- [x] `feat(state): card and agent rail persistence`
- [x] `feat(api): snooze wake commit`
- [x] `feat(ui): hydrate on load`

---

## M5 — Suzy coordinator + router

**Goal:** LLM routing; dynamic summaries; server-driven tour.

### Tasks

- [ ] **M5-T1** `src/agents/suzy.rs` — coordinator `LlmAgent`
- [ ] **M5-T2** `src/agents/router.rs` — `LlmConditionalAgent` for 7 scenarios
- [ ] **M5-T3** Remove static `suzySummaries{}` in production mode
- [ ] **M5-T4** SSE `suggest` events for guided tour
- [ ] **M5-T5** Audio: match summary hash → existing `.wav`; else queue `gen_audio.py`
- [ ] **M5-T6** Router fallback when intent is ambiguous (clarifying question)

### Validation

- [ ] "pitch presentation" routes to deck
- [ ] Suzy summary changes when underlying session data changes
- [ ] Tour suggestions advance after each scenario

### Commits

- [ ] `feat(agents): suzy coordinator + llm router`
- [ ] `feat(ui): server-driven tour suggestions`

---

## M6 — Remaining scenarios

**Goal:** live, people, week, lisbon — real MCP data.

**MCP Phases C–F:** see spec §11.1

### Tasks — Live (Phase C)

- [ ] **M6-T1** `live_workflow` — Headlines, Markets, Now cards
- [ ] **M6-T2** `mcp-news` for headlines + now; optional `mcp-market-data` for portfolio

### Tasks — People (Phase D)

- [ ] **M6-T3** `people_workflow` — Team, Priya, Connections
- [ ] **M6-T4** `mcp-slack` + `mcp-crm` + `mcp-calendar` (1:1 prep)

### Tasks — Week (Phase E)

- [ ] **M6-T5** `week_workflow` — Money, Health, Focus
- [ ] **M6-T6** `mcp-banking` for Money; `mcp-github` for Focus
- [ ] **M6-T7** Health — manual CSV/HealthKit import adapter (BK-010)

### Tasks — Lisbon (Phase F)

- [ ] **M6-T8** `lisbon_workflow` — Itinerary via `mcp-maps` + `mcp-weather`
- [ ] **M6-T9** Stay scout via `mcp-real-estate` (partial)
- [ ] **M6-T10** Flights — `computer-use-mcp` fallback OR labeled stub (BK-009)
- [ ] **M6-T11** Shell rails wired: People (`mcp-slack`), Live (`mcp-news`)

### Validation

- [ ] Full `tourOrder` — each scenario has MCP log / artifact / API evidence
- [ ] Lisbon flights/stay labeled if not fully automated (no silent fake data)

### Commits

- [ ] `feat(agents): live people week lisbon workflows`
- [ ] `feat(mcp): phases C through F`

---

## M7 — Ambient proactive layer

**Goal:** Background agents + AWP event webhooks.

**MCP Phase G:** mcp-news · mcp-real-estate · mcp-search

### Tasks

- [ ] **M7-T1** `AmbientAgent` × 3 — research, scout, maker
- [ ] **M7-T2** Cron triggers for research + scout
- [ ] **M7-T3** `GET /api/ambient` SSE — rail tile updates
- [ ] **M7-T4** `user:dnd` flag suppresses proactive card bloom
- [ ] **M7-T5** AWP `POST /awp/events/subscribe` — webhook on proactive completion (FR-086)
- [ ] **M7-T6** Optional `mcp-notifications` push
- [ ] **M7-T7** `business.toml` — `subscribe_proactive` capability

### Validation

- [ ] Trigger cron (or wait 30 min) — proactive rail updates
- [ ] "Show me what you found" blooms from stored ambient results
- [ ] AWP webhook subscriber receives signed event on scout hit

### Commits

- [ ] `feat(ambient): proactive agents + cron`
- [ ] `feat(awp): event subscriptions for proactive`

---

## M8 — Shell rails polish

**Goal:** Rails + background cards server-driven; remove hardcoded demo data in production mode.

### Tasks

- [ ] **M8-T1** `GET /api/people` — `mcp-slack` users/DMs
- [ ] **M8-T2** `GET /api/live` — `mcp-news` carousel slides
- [ ] **M8-T3** Background card intents from server config (replace `bgcards[]` hardcode)
- [ ] **M8-T4** Production mode banner — demo mode labeled when `?demo=1`
- [ ] **M8-T5** Remove silent fallback to `scenarios{}` except explicit demo flag

### Validation

- [ ] Slack status change → People rail updates
- [ ] Live carousel shows live headlines (not static `sources[]`)

### Commits

- [ ] `feat(api): people and live rails`
- [ ] `feat(ui): server-driven background cards`

---

## M9 — Auth + Postgres

**Goal:** Multi-device sessions; user-scoped artifacts.

### Tasks

- [ ] **M9-T1** `migrations/` — users, sessions, agent_events (from excel-agent-app pattern)
- [ ] **M9-T2** `src/pg_session.rs` — Postgres `SessionService`
- [ ] **M9-T3** Google OAuth (minimum); JWT cookies
- [ ] **M9-T4** Artifacts scoped to `user_id`
- [ ] **M9-T5** AWP trust levels tied to auth tier (known vs anonymous)
- [ ] **M9-T6** `mcp-registry` allowlists per sub-agent (BK-011)
- [ ] **M9-T7** Rate limiting on `/api/intent` (BK-006)

### Validation

- [ ] Login on two browsers — same session shows same cards/rails
- [ ] Anonymous AWP call to gated capability → error envelope

### Commits

- [ ] `feat(db): postgres sessions`
- [ ] `feat(auth): google oauth + jwt`

---

## M10 — Live voice Suzy

**Goal:** Gemini Live bidirectional audio; clips remain fallback.

### Tasks

- [ ] **M10-T1** `src/voice/realtime.rs` — `adk-realtime` runner
- [ ] **M10-T2** `WS /ws/voice` — mic PCM in, audio PCM out (mia pattern)
- [ ] **M10-T3** Tool calls during voice session
- [ ] **M10-T4** Fallback to prerecorded `.wav` on WS failure
- [ ] **M10-T5** Greeting screen optional live voice path

### Validation

- [ ] Voice session with Suzy updates session state in real time
- [ ] Clips still work when WS unavailable

### Commits

- [ ] `feat(voice): gemini live websocket`

---

## M11 — Deployed prototype

**Goal:** Public HTTPS URL; AWP conformance; CI.

### Tasks

- [ ] **M11-T1** `Dockerfile` + `deploy/Caddyfile`
- [ ] **M11-T2** GitHub Actions — `cargo test`, `cargo clippy`
- [ ] **M11-T3** `capture.js` targets production URL
- [ ] **M11-T4** `.env.example` — all MCP paths + API keys documented
- [ ] **M11-T5** AWP conformance test against production URL
- [ ] **M11-T6** Early-access footer links `/.well-known/awp.json`
- [ ] **M11-T7** LinkedIn conversion still fires (BK-008)

### Validation

- [ ] `https://…` passes full tour desktop + mobile Safari
- [ ] `POST /awp/a2a` deck scenario without browser
- [ ] Regression suite (below) all green

### Commits

- [ ] `feat(deploy): docker caddy production`
- [ ] `ci: test and clippy`

---

## Cross-cutting backlog

- [ ] **BK-001** Playwright screenshot diff — card bloom (M0+)
- [ ] **BK-002** SSE event schema types — contract tests (M0+)
- [ ] **BK-003** `business.toml` capabilities stay in sync with routes (ongoing)
- [ ] **BK-004** Deck integration test — three artifacts (M1)
- [ ] **BK-005** Tool allowlist per sub-agent (M1)
- [ ] **BK-006** Rate limiting (M9)
- [ ] **BK-007** `gen_audio.py` in CI when copy changes (M5)
- [ ] **BK-008** LinkedIn conversion after API mode (M11)
- [ ] **BK-009** `mcp-travel` or permanent computer-use path for flights (M6)
- [ ] **BK-010** Health data ingest — HealthKit/CSV (M6)
- [ ] **BK-011** `mcp-registry` integration (M9)
- [ ] **BK-012** A2A message type routing table (M0)
- [ ] **BK-013** Manifest sync script — `business.toml` ↔ routes (ongoing)
- [ ] **BK-100** Run and record the M0–M11 validation scripts (statuses above are code-derived)

---

## Regression suite (run after M2, every release)

- [ ] Greeting → start day → scenario blooms
- [ ] Deck → combine → pinned `.pptx` artifact
- [ ] `?demo=1` offline simulation works
- [ ] Early-access signup POST succeeds
- [ ] Mobile ≤900px — stacked layout, no coverflow break
- [ ] Voice input submits intent (Chrome)
- [ ] `/.well-known/awp.json` valid
- [ ] `cargo test` passes
- [ ] `cargo clippy` clean

---

## v1.0 definition of done

- [ ] All milestones M0–M11 merged and tagged
- [ ] All tasks above checked (or explicitly deferred with issue link)
- [ ] FR-001–FR-087 satisfied ([`SPECIFICATION.md`](./SPECIFICATION.md))
- [ ] NFR-001–NFR-011 satisfied
- [ ] No production path uses static `scenarios{}` or fake `setInterval` streams
- [ ] Demo mode labeled "simulated" in UI
- [ ] AWP reference demo documented in Appendix D

---

## Quick start (M0 first sprint)

```bash
cd /Users/jameskaranja/Developer/projects/spatial-os
git checkout -b milestone/M0-live-shell

# Build MCP binaries (needed from M1; optional smoke in M0)
# (cd ../mcp-servers/docx-mcp && cargo build --release)
# (cd ../mcp-servers/mcp_slides && cargo build --release)

cargo run
open http://localhost:8080
curl http://localhost:8080/.well-known/awp.json | jq .
```

---

*Update the Progress summary table when each milestone merges.*