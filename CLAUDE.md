# Agentrix OS — working notes for developers using Claude Code

Read this before changing anything. It is the project's operating manual for AI-assisted work:
how to build, how to test, where things live, and the rules that must survive every change.
The full product spec is `docs/SPECIFICATION.md`; the Phase 2 concept is `docs/PERSONAL_AI_OS.md`
and its delivery plan `docs/SPRINT_PLAN.md`.

## What this repo is

`spatial-os` is a Rust/Axum server plus a single-page field UI. A user states an intent (text or
voice); an LLM router picks a scenario; a workflow of `adk-rust` agents runs against MCP toolservers
(calendar, email, news, weather, Slack, CRM, banking, GitHub, maps, real estate, spreadsheets,
documents, slides); results stream to the browser as cards over SSE; Suzy summarizes. External
agents reach the same pipeline through the Agentic Web Protocol (`/.well-known/awp.json`,
`/awp/manifest`, `/awp/a2a`). Phase 2 turns this into a Personal AI OS: one Mother Agent,
a Work World and a Home World, an intelligence layer, Observe / Suggest / Automate permissions,
and user-owned memory.

## Build and run

- **Sibling checkout required.** `Cargo.toml` uses path dependencies on `../adk-rust/*`.
  Clone `https://github.com/zavora-ai/adk-rust` next to this repo before `cargo` anything.
- `cp .env.example .env && cargo run` → http://localhost:9847. Add `?demo=1` for the offline,
  scripted UI (no server events). Keep demo mode working; marketing captures depend on it.
- Everything degrades honestly: no `GOOGLE_API_KEY` → scenarios stream their mocks; no MCP
  binaries → labeled stubs; no `DATABASE_URL` → in-memory sessions. Never make a feature *require*
  a live integration to compile or boot.
- MCP servers are separate binaries built from the `mcp-servers` monorepo; paths are the
  `MCP_*_PATH` variables in `.env.example`. Postgres for local dev: `docker compose up -d`
  (port 5434); migrations run at boot via `sqlx::migrate!`.
- **A shell-exported `GOOGLE_API_KEY` wins over `.env`.** dotenv never overrides a variable that
  is already set, so a stale `export GOOGLE_API_KEY=…` in `~/.zshrc` makes the server and the
  live tests use that key while `.env` says otherwise (symptom: `API key not valid` from Gemini
  although `curl` with the `.env` key works). Run `unset GOOGLE_API_KEY` first, or pass
  `GOOGLE_API_KEY=$(grep '^GOOGLE_API_KEY=' .env | cut -d= -f2) cargo run`.
- **adk-rust drift.** adk-rust `main` moves fast. As of 2026-09-19 this repo's `main` does not
  compile against it (rmcp 3, `AwpState::builder`, rate-limiter signature). The fix is the first
  commit of PR #2 (`build: track adk-rust main`). If you hit type mismatches in `src/tools/mcp.rs`
  or `AwpState`, start there rather than pinning rmcp locally.

## Test and lint

```bash
cargo test --lib                                  # unit tests (fast, offline)
cargo test --test validate -- awp_gate business_toml mock_deck combine_action keyword_router \
  artifact_paths session_store voice_state greeting_brand background_cards ambient_store \
  tour_prompts proactive_mock mcp_allowlist health_exposes \
  domain_ intake_ mother_ ledger_ permission_ gate_ pending_ memory_ effects_ \
  consent_ tasks_ s7_                    # same list as CI; DB-backed ones return early without DATABASE_URL
cargo test --test validate -- gemini_ router_agent suzy_agent                             # needs GOOGLE_API_KEY
cargo test --test validate -- postgres_schema ui_session_persists pg_agent_session        # needs DATABASE_URL
cargo test --test validate deck_workflow_writes_three_artifacts -- --ignored              # full E2E, slow
cargo clippy --all-targets
```

- **libtest filters are substrings, not regexes.** Pass several names after `--`; a single
  `'a|b|c'` argument matches nothing (the CI on `main` still has that mistake; PR #2 fixes it).
- Tests that need secrets or binaries panic with a clear message when they are missing; do not
  add them to the offline group.
- Keep new code clippy-clean. Warnings that already exist in `greeting/`, `auth.rs`,
  `ambient/service.rs` are known; do not churn unrelated files to silence them.
- Before opening a PR: `cargo check --all-targets`, the offline test group, clippy, and a
  manual run of the demo tour (`?demo=1`) plus one live scenario if you touched orchestration.

## Layout

| Path | What lives there |
|---|---|
| `src/main.rs` | Boot: config, MCP spawn per phase, agent runners, AWP state, routes. Read it first. |
| `src/config.rs` | Every env var, with defaults. Add new settings here, then to `.env.example`. |
| `src/state.rs` | `AppState` (runners, MCP pools, stores) and `SessionStore` (in-memory + Postgres `ui_sessions`). |
| `src/agents/` | One file per scenario workflow or card agent. `gemini.rs` sanitizes tool schemas and applies `mcp_allowlists.toml` via `filtered_for_agent`. `stub.rs` makes labeled stubs. `router.rs` and `suzy.rs` are the Phase 1 coordinator. |
| `src/orchestrator/` | `dispatch.rs` routes an intent to a scenario stream; `workflow.rs` is the generic SSE streamer for multi-card workflows; `coordinator.rs` emits Suzy summaries and tour suggestions; one file per scenario for cards and resolve shapes. |
| `src/events/sse.rs` | `FieldEvent` — the SSE contract with the UI. `mock.rs` streams scripted scenarios when live agents are off. |
| `src/routes/` | Axum handlers. Each public route has a `[[capabilities]]` entry in `business.toml` and calls `state.awp.check(...)`. |
| `src/tools/` | MCP child-process spawn with reconnect (`mcp.rs`), allowlist catalog (`allowlist.rs`), registry sync, `MergedToolset`, `exec_tool` for one-off calls. |
| `src/ambient/` | Cron-driven background agents (research, scout, maker), `AmbientStore` with a broadcast channel and the DND flag. |
| `src/intelligence/` | Content-free ledger (`ledger.rs`), daily pattern aggregation (`patterns.rs`, S7-T1), personal baseline and drift detection (`baseline.rs`, S7-T2), their Postgres I/O (`store.rs`). Pure statistics, no clock: `as_of` is an argument. `src/bin/run_baseline.rs` runs the pipeline locally; `INTELLIGENCE_UTC_OFFSET_HOURS` sets the day boundary. |
| `src/rails/`, `src/greeting/`, `src/voice/` | People/Live rails, personalized greeting (facts only), Gemini Live voice over WebSocket; `voice/camera.rs` is the camera channel (frames in, `ui_gesture` tool call out, `AGENTRIX_CAMERA`). |
| `src/awp_gate.rs`, `src/auth.rs`, `src/pg_session.rs` | AWP trust levels and rate limits, JWT/OAuth, adk session persistence. |
| `web/index.html`, `web/static/field-client.js` | The field UI and its SSE bridge. The visual language is a product constraint — do not redesign it while wiring features. |
| `business.toml`, `mcp_allowlists.toml` | AWP identity and capability catalog; per-agent tool allowlists (and, from Phase 2, world / mode / effects). |
| `migrations/` | sqlx migrations, applied at boot. Numbered; never edit an applied one. |
| `docs/` | `SPECIFICATION.md` (Phase 1 spec, FR/NFR ids), `IMPLEMENTATION_PLAN.md` (tickable milestones and sprints), `PERSONAL_AI_OS.md` + `SPRINT_PLAN.md` (Phase 2), `PROGRESS.md` (per-person team sprints A–F for S4–S12), `adr/` (decisions), `personal-ai-os.html` (one-page interactive summary). |
| `tests/validate.rs` | The integration suite; `tests/common/mod.rs` has env and path helpers. |
| `scripts/` | Demo capture, TTS clip generation, synthetic ledger generator. |

## Rules that must survive every change

1. **Honest data.** Never fabricate facts in prose, cards or summaries. If an integration is
   missing, use `agents::stub::labeled_stub` and say so in the UI. The greeting agent composes
   only from `INTEGRATION_FACTS`; keep that discipline for anything user-facing.
2. **Least privilege.** Every sub-agent receives only the tools listed for its id in
   `mcp_allowlists.toml`, through `gemini::filtered_for_agent`. Adding an agent starts with an
   allowlist entry. Never mount a whole MCP server on a coordinator.
3. **SSE contract is shared.** New server → client events are `FieldEvent` variants, handled in
   `field-client.js`, and documented in `SPECIFICATION.md` §7.5. Do not invent ad-hoc JSON.
4. **Capabilities stay in sync with routes** (BK-003). New public route → `business.toml`
   entry with the right `access_level` → `state.awp.check` in the handler.
   `known` for anything that acts, changes settings, or reads personal data.
5. **Sessions persist through `SessionStore`.** New persisted fields need `#[serde(default)]`,
   a migration, and updates to `upsert_ui_session` / `load_ui_session` in `state.rs`.
6. **Demo mode stays alive** (NFR-008) and is labeled as simulated.
7. **Preserve the design.** Wire behaviour behind the existing UI; do not restyle `index.html`.

## Phase 2 — Personal AI OS

Concept `docs/PERSONAL_AI_OS.md`, plan `docs/SPRINT_PLAN.md` (13 sprints, S0–S12, four releases),
decisions in `docs/adr/`. Release R1 (S0–S3) is in **PR #2** on branch `phase2/r1-foundation`.
Once it merges, these rules apply in addition to the ones above:

- **Domain on everything** (ADR-002). `Domain { Work, Home, Shared }` tags cards, agents,
  `card_spawn`, ledger rows, memory items and pending actions. `Shared` is the backward-compatible
  default. Home agents never read `work.*` memory and vice versa; only the Mother and Balance
  agents read across, through the scoped API.
- **The Mother Agent is the only entry point** (ADR-001). Intent, chat, voice and `/awp/a2a`
  all call `mother::handle_intent`. It has no MCP tools; delegation goes through
  `orchestrator::dispatch::stream_scenario` (Phase 1 scenarios stand in for world agents until
  S4/S5). Multi-target turns merge cards and end with one synthesis.
- **Agent contract** (ADR-003). Each `[[allowlist]]` entry declares `world`, `mode`
  (`observe | suggest | automate`) and an `[allowlist.effects]` table mapping every tool to
  `read | write_local | schedule_with_others | send_external | publish_public | financial | delete`.
  Boot fails on an unclassified tool. Unknown tools are treated as `send_external`.
- **Never bypass the permission gate.** `filtered_for_agent` wraps every toolset in
  `permissions::PermissionGate`; reads pass, `write_local` runs in suggest/automate, other
  effects queue a pending action, `publish_public` and `financial` are never automated, observe
  denies with an explanation, pause blocks writes. Every decision is audited and ledgered.
  Approvals execute through `permissions::execute_approved` with the user as the authority.
- **The ledger is content-free** (ADR-004). `activity_events` holds kinds, domains, effects,
  durations, hashed subjects and an allow-listed `meta`. Never put bodies, subjects, names or
  argument values in it, in audit summaries, or in tracing output.
- **Memory kinds.** Agents may only `propose_memory` (stored as `assumed`); only the user makes
  something `known` (statement, confirmation, correction). Synthesis cites `(you told me)` /
  `(I think)`. Sensitive values are encrypted at rest with `MEMORY_MASTER_KEY`.
- **Neutral language.** Observations state the number, the period and the baseline. No verdicts,
  no guessed causes, no moralizing (`PERSONAL_AI_OS.md` §7.6).
- **Where things live (R1):** `src/mother/{intake,delegate,synth,agent}.rs`,
  `src/permissions/{gate,store,pending,audit}.rs`, `src/intelligence/ledger.rs`,
  `src/memory/{service,crypto,tools,chat}.rs`, routes `chat`, `actions`, `permissions`, `events`,
  `memory`; migrations 004 (chat history), 005 (ledger), 006 (permissions), 007 (memory).
- **Testing gotchas.** The permission and memory services are process-wide (`permissions::gate::services()`,
  `memory::service_handle()`), so integration tests share them; `SimpleToolContext` reports the
  user as `anonymous`, so gate tests serialize on `gate_lock()`. Use the `offline_app_state()`
  and `known_app_state()` fixtures in `tests/validate.rs` for handler tests. Ledger in-memory
  writes are synchronous; only Postgres inserts are async (`ledger.flush()` waits for them).
- **Next sprints:** S4 Work World (`work_mother`, tasks store), S5 Home World (family, personal
  productivity, separate identity), S6 agent bus + daily briefing. Use the task ids from
  `SPRINT_PLAN.md` (for example `S4-T3`) in commit messages and PR descriptions.

## Delivery ritual

1. Branch per milestone or sprint: `milestone/Mx-name` (Phase 1) or `phase2/Sxx-name`.
2. Work the task checklist; keep `cargo check`, the offline tests and clippy green.
3. Record the validation script from `IMPLEMENTATION_PLAN.md` (screen capture); the product owner
   runs it and signs off before merge.
4. Merge to `main`, tag (`v0.Mx.0` / `v1.x.0-p2`), update the progress table in
   `IMPLEMENTATION_PLAN.md` and the capability list in `business.toml`.
5. Commits: `feat(scope): …`, `fix(scope): …`, `docs: …`, `build: …`. Imperative subject, a body
   that says why, task ids where they apply. No generated-by footers or co-author trailers.

## When asking Claude Code for changes

- Point it at the relevant spec section (`SPECIFICATION.md` §5–§7 or `PERSONAL_AI_OS.md` §N) and
  the sprint task id; the plan already lists proposed file paths and acceptance checks.
- Ask for the smallest change that fits an existing pattern: a `FieldEvent` variant, a toolset
  wrapper, a `SessionStore::mutate`, an allowlist entry, a labeled stub.
- Ask it to run the offline test group and clippy before declaring done, and to say plainly what
  it could not exercise (no API key, no Postgres, no MCP binaries) instead of implying coverage.
- It must not weaken the seven rules above to make something work; if a rule blocks the task,
  surface the conflict in the PR description.
