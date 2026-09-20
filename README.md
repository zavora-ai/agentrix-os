# Agentrix OS

**The agentic operating system that works while you live.** One person, one AI OS, one Mother Agent, many specialised agents, one coordinated life.

[![CI](https://github.com/zavora-ai/agentrix-os/actions/workflows/ci.yml/badge.svg)](https://github.com/zavora-ai/agentrix-os/actions/workflows/ci.yml)
[![Built with Claude Fable 5.1](https://img.shields.io/badge/built%20with-Claude%20Fable%205.1-7c5cff)](https://www.anthropic.com/claude/fable)
[![Claude Fable 5.1 Build · Hackerhouse Africa](https://img.shields.io/badge/Claude%20Fable%205.1%20Build-Hackerhouse%20Africa%20%C2%B7%20Nairobi%2C%20Kenya-25d0c0)](#built-with-claude-fable-51)

> ### Built with Claude Fable 5.1
> Agentrix OS was built at the **Claude Fable 5.1 Build** at **Hackerhouse Africa, Nairobi, Kenya**, with **Claude Fable 5.1** as the engineering partner on every lane — through Claude Code, from the Phase 2 concept and sprint plan to the Rust services, the browser UI, the tests and the verification runs recorded in each pull request. The product's text agents are moving onto Claude Fable 5.1 as well, with Gemini as the fallback ([#19](https://github.com/zavora-ai/agentrix-os/pull/19)).

Agentrix OS is a Rust/Axum server and a spatial browser UI. You state an intent, by text or voice. A **Mother Agent** classifies it, delegates to specialised agents organised into a **Work World** and a **Home World**, runs them against real tools through MCP servers, and streams the results to the field as cards. Suzy, its voice, composes one answer. Every action an agent wants to take passes a **permission gate** you control, and everything the system knows about you lives in a **memory you can read, correct, export and delete**.

![Agentrix OS — the Work world with eight agents and live cards](docs/screenshots/02-work-world.jpg)

## Contributors

Four people, four lanes, one AI pair. Areas are what each has landed on `main` or has in review; the lane model is in [`docs/PROGRESS.md`](docs/PROGRESS.md).

| Contributor | Lane | What they built |
|---|---|---|
| **James Karanja Maina** · [@jkmaina](https://github.com/jkmaina) | Tech lead, product owner · **Platform & data** | Phase 1 end to end (M0–M11): the live SSE shell, the real deck workflow over MCP, the seven scenarios, ambient agents on cron, Postgres sessions and JWT auth, Gemini Live voice, the Docker + Caddy deploy. Phase 2: the team plan and ADRs 005–006, the tasks store and Productivity tools, persisted consents, migrations, a CI that actually runs, the runner-session fix that made the Mother's LLM paths work, the voice fix from the mia pattern, the Agentrix rebrand, the world rosters, local camera gestures, drag and swipe fixes, this README. |
| **Kim** · [@swiftkimani](https://github.com/swiftkimani) | **Agents & worlds** | The Personal AI OS concept, the sprint plan and the one-page overview; `CLAUDE.md`; Phase 2 Release R1 — Mother Agent intake, delegation and synthesis, the content-free activity ledger, the permission gate with pending actions and audit, personal memory with provenance and encryption; the Work World and Home World mothers and agent registries, follow-up tracker, health guardrail and labeled stubs; the agent bus and arbitration v1; the camera channel that lets Suzy see. |
| **Robert Kimaiyo** · [@Robertkip](https://github.com/Robertkip) | **Surface** | The two themed worlds with swipe navigation and the domain lens, mode badges on cards and tiles, the Mother chat panel, the approvals inbox with live expiry and the away feed, content-free UI signals for the ledger, TypeScript types for the SSE contract. |
| **Jotham Siror** · [@Jotham-Siror](https://github.com/Jotham-Siror) | **Intelligence** | Daily pattern aggregation over the ledger and the personal baseline — median and MAD, weekday and weekend classes, 14-day warm-up, drift detection — with the synthetic six-week drift fixture; running the text agents on Claude Fable 5.1 with Gemini fallback (in review). |
| **Claude Fable 5.1** · [Anthropic](https://www.anthropic.com/claude/fable) | **AI pair on every lane** | Concept and plan review, ADR drafting, Rust and JavaScript implementation, test suites, headless browser verification, live agent and MCP verification, and the pull request write-ups — driven by the team through Claude Code at the Hackerhouse Africa build. |

## Contents

- [Contributors](#contributors)
- [Status](#status)
- [Architecture](#architecture)
- [Screenshots](#screenshots)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [MCP servers](#mcp-servers)
- [API](#api)
- [Voice and camera](#voice-and-camera)
- [Testing](#testing)
- [Project layout](#project-layout)
- [Documentation](#documentation)
- [Team workflow](#team-workflow)

## Status

| Phase | What | State |
|---|---|---|
| **Phase 1 · M0–M11** | Live SSE shell, real deck workflow (Excel · Word · PowerPoint through MCP), morning / people / week / Lisbon / live scenarios, ambient agents on cron, Postgres sessions + JWT auth, Gemini Live voice, Docker + Caddy deploy | on `main` |
| **Phase 2 · R1 (S0–S3)** | Mother Agent, domain model, content-free activity ledger, permission gate with pending actions and audit, personal memory with provenance and encryption | on `main` |
| **Phase 2 · team sprint A** | Work World and Home World mothers, agent registries and rosters, tasks store, persisted consents, daily patterns + personal baseline, chat panel, approvals inbox, worlds pager, camera channel | on `main` / in review |
| **Next** | Agent bus journeys, daily briefing v2, balance and behaviour observations, TODAY command centre, trust centre, recipes, public beta | see [`docs/PROGRESS.md`](docs/PROGRESS.md) |

Everything degrades honestly: without a Gemini key the scenarios stream scripted mocks, without an MCP binary an agent ships as a **labeled stub**, without a database the stores are in-memory. Nothing is ever fabricated to look connected.

## Architecture

![Architecture — surfaces, Mother Agent, two worlds, intelligence, memory, automation and trust, privacy, integrations](docs/architecture.svg)

The layers, top to bottom. Each row links to the section of the concept document that specifies it.

| Layer | Responsibility | Where |
|---|---|---|
| **Surfaces** | The spatial field (cards, VerbUI gestures, worlds pager), the Mother chat panel, the approvals inbox, Suzy's live voice and camera, and the Agentic Web Protocol endpoint for external agents | `web/`, `src/routes/`, `src/voice/` |
| **Mother Agent** ([§3](docs/PERSONAL_AI_OS.md#3-mother-agent)) | Intake → context assembly → delegation → arbitration → synthesis → action gate. Owns no MCP tools; its tools are internal, so a wrong routing decision can never send an email | `src/mother/` |
| **Worlds** ([§4](docs/PERSONAL_AI_OS.md#4-work-world), [§5](docs/PERSONAL_AI_OS.md#5-home-world)) | A **Work Mother** over Productivity, Email, Team Comms, Project, Research & Knowledge, Work Automation, Career, Professional Social. A **Home Mother** over Family, Personal Productivity, Health & Wellness, Finance (read-only), Entertainment, Social & Fun, Travel, Personal Social. Only the Mother and the Balance Agent see both | `src/worlds/`, `GET /api/worlds` |
| **Agent contract** ([§6](docs/PERSONAL_AI_OS.md#6-specialized-agents)) | Every agent declares its world, mission, MCP allowlist with an **effect class per tool**, default authority mode and memory scope in `mcp_allowlists.toml` | `src/tools/allowlist.rs` |
| **Intelligence** ([§7](docs/PERSONAL_AI_OS.md#7-intelligence-layer), [§8](docs/PERSONAL_AI_OS.md#8-personal-baseline--going-off-the-book)) | A content-free activity ledger feeds deterministic pattern aggregation and a personal baseline (median + MAD, 14-day warm-up, drift detection). The model only phrases observations, in neutral language | `src/intelligence/` |
| **Memory** ([§11](docs/PERSONAL_AI_OS.md#11-memory-architecture)) | Three kinds, always labeled: **known** (you said it), **assumed** (inferred), **recommended**. Domain-scoped, with provenance, consent-linked, encrypted when sensitive | `src/memory/` |
| **Automation & trust** ([§10](docs/PERSONAL_AI_OS.md#10-automation-layer)) | Each agent runs in **Observe**, **Suggest** or **Automate**. A gate wraps every toolset and decides by effect class; non-local effects in Suggest become pending actions you approve, and every decision is audited | `src/permissions/` |
| **Privacy & security** ([§12](docs/PERSONAL_AI_OS.md#12-privacy--security)) | Identity (JWT, Google OAuth), per-agent tool allowlists, domain isolation, consents, encryption at rest, content-free logs | `src/auth.rs`, `src/awp_gate.rs` |
| **Integrations** | Fourteen MCP toolservers spawned as child processes, plus AWP peers | `src/tools/mcp.rs` |

### The permission gate in one table

| Effect class | Examples | Observe | Suggest | Automate |
|---|---|---|---|---|
| `read` | list inbox, today's events | runs | runs | runs |
| `write_local` | create a draft, save a deck, create a task | denied | runs | runs |
| `schedule_with_others`, `send_external`, `delete` | create an event with attendees, send a reply, archive | denied | **pending action** | only through an approved recipe |
| `publish_public`, `financial` | post publicly, move money | denied | pending, with confirmation | **never** |

### Design principles

Orchestrate, don't chat · two worlds, one life · inform and suggest, never control · neutral language · never assume why · least privilege everywhere · human in control · honest data · memory is the user's · health and money stay conservative. The full list, with the reasoning behind each, is in [§1.3 of the concept](docs/PERSONAL_AI_OS.md#13-design-principles).

## Screenshots

<table>
<tr>
<td width="50%"><img src="docs/screenshots/01-greeting.jpg" alt="Greeting screen"><br><sub><b>Greeting.</b> Suzy composes the opening line from connected integrations only; with none connected she says so.</sub></td>
<td width="50%"><img src="docs/screenshots/02-work-world.jpg" alt="Work world"><br><sub><b>Work world.</b> The eight work agents with their authority modes on the right; cards from the morning and people flows; labeled stubs where an integration does not exist yet.</sub></td>
</tr>
<tr>
<td><img src="docs/screenshots/03-home-world.jpg" alt="Home world"><br><sub><b>Home world.</b> Swipe, scroll, arrow keys or the pager; the theme, the roster and the visible cards change with the world.</sub></td>
<td><img src="docs/screenshots/04-mother-chat.jpg" alt="Mother chat"><br><sub><b>Mother chat.</b> Multi-turn; "remember …" becomes a <i>known</i> memory item with provenance, cited as "(you told me)".</sub></td>
</tr>
<tr>
<td><img src="docs/screenshots/05-deck-artifacts.jpg" alt="Deck workflow"><br><sub><b>Real work.</b> "Build me a pitch deck" runs Excel, Word and PowerPoint agents against real MCP servers and links three downloadable files.</sub></td>
<td><img src="docs/screenshots/06-approvals.jpg" alt="Approvals inbox"><br><sub><b>Approvals.</b> Anything an agent may not do on its own waits here; approve, edit or reject, singly or in batch. Sign-in required.</sub></td>
</tr>
</table>

## Quick start

**Prerequisites**

- Rust 1.95 or newer (the pinned toolchain).
- A sibling checkout of [adk-rust](https://github.com/zavora-ai/adk-rust) on `main` at `../adk-rust`. `Cargo.toml` uses path dependencies.
- Optional: the [mcp-servers](https://github.com/zavora-ai/mcp-servers) monorepo at `../mcp-servers` for real tools. Each server builds on its own in about half a minute.
- Optional: a Gemini API key for live agents and voice, and Docker for Postgres.

```bash
git clone https://github.com/zavora-ai/agentrix-os.git
git clone https://github.com/zavora-ai/adk-rust.git      # sibling
cd agentrix-os
cp .env.example .env                                    # add GOOGLE_API_KEY for live agents
cargo run
open http://127.0.0.1:9847
```

Add `?demo=1` for the offline, scripted tour. For the real deck flow, build the three document servers and point the `MCP_*_PATH` variables at them:

```bash
for s in worksheet-mcp docx-mcp mcp-slides; do (cd ../mcp-servers/$s && cargo build --release); done
```

For persistence and sign-in, start Postgres and set `DATABASE_URL` and `JWT_SECRET`:

```bash
docker compose up -d        # Postgres on port 5434; migrations run at boot
```

One gotcha: dotenv never overrides a variable your shell already exports. If `~/.zshrc` exports an old `GOOGLE_API_KEY`, the server uses that one, not `.env`.

## Configuration

All settings are environment variables, read in `src/config.rs` and documented in `.env.example`.

| Variable | Purpose | Default |
|---|---|---|
| `GOOGLE_API_KEY` | Gemini for agents, the Mother and live voice; unset → scripted mocks | unset |
| `GEMINI_MODEL` | Text model for agents | `gemini-3.1-flash-lite` |
| `GEMINI_LIVE_MODEL`, `VOICE_NAME` | Live voice model and voice | `models/gemini-3.8-live`, `Aoede` |
| `AGENTRIX_CAMERA` | Camera channel over the voice socket | `1` |
| `MCP_*_PATH` | Binaries for worksheet, docx, slides, calendar, email, news, weather, market data, Slack, CRM, banking, GitHub, maps, real estate | `../mcp-servers/<crate>/target/release/<bin>` |
| `DATABASE_URL`, `JWT_SECRET` | Postgres persistence and sign-in; both required for a Phase 2 deployment ([ADR-006](docs/adr/006-phase2-requires-signed-in-user-and-postgres.md)) | unset → in-memory, anonymous |
| `GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET` | Sign in with Google; without them use the dev sign-in | unset |
| `MEMORY_MASTER_KEY`, `LEDGER_HASH_KEY` | Encrypt sensitive memory values; hash ledger subjects | dev keys, with a warning |
| `AGENTRIX_WORK_MOTHER`, `AGENTRIX_HOME_MOTHER` | Set to `0` to fall back to direct per-agent fan-out | on |
| `AGENTRIX_ALLOW_DEMO`, `BASE_URL`, `AGENTRIX_SIGNUP_ENDPOINT` | Demo mode off-localhost, public URL, early-access form | see `.env.example` |

## MCP servers

Agents reach the outside world only through MCP toolservers, spawned as child processes at boot and health-checked. Each agent receives only the tools listed for it in `mcp_allowlists.toml`, and each tool carries an effect class the permission gate uses.

| Server | Used by | Credentials |
|---|---|---|
| worksheet, docx, slides | deck workflow, Work Automation | none |
| news, weather, maps, market data | morning brief, live, Lisbon, entertainment | none (GNews headlines need `GNEWS_API_KEY`) |
| calendar | Productivity, Today card | `GOOGLE_CALENDAR_TOKEN` or `mcp-calendar auth google` |
| email | Email agent, Needs-you card | Gmail OAuth (`mcp-email auth gmail`) or SMTP/IMAP |
| Slack, CRM, GitHub | Team Comms, Project | `SLACK_BOT_TOKEN`, a CRM token, `GITHUB_TOKEN` |
| banking, real estate | Finance (read-only), Travel | Plaid/Mono, Attom/Bayut |

A server that cannot start leaves its agent as a labeled stub in the UI rather than a fake card.

## API

Every public route has a `[[capabilities]]` entry in `business.toml` with an access level, enforced by the AWP gate. `anonymous` routes serve the tour and static catalogs; anything that acts, changes settings or reads personal data is `known` (signed in).

**Sessions and intents**
`POST /api/sessions` · `POST /api/sessions/{id}/intent` (SSE) · `POST /api/sessions/{id}/chat` (SSE through the Mother; `GET` returns the transcript) · `POST /api/sessions/{id}/action` · `POST /api/sessions/{id}/events` (content-free UI signals)

**Field state**
`GET /api/sessions/{id}/cards` · `GET /api/sessions/{id}/agents` · `POST /api/agents/{id}/snooze|wake` · `POST /api/sessions/{id}/commit` · `POST /api/sessions/{id}/fuse`

**Trust**
`GET /api/actions` · `POST /api/actions/{id}/approve|reject|edit` · `POST /api/actions/approve` (batch) · `GET /api/audit` · `GET/PUT /api/permissions` · `POST /api/pause` · `POST /api/resume` · `GET/PUT /api/consents`

**Memory and tasks**
`GET/POST/DELETE /api/memory` · `PATCH/DELETE /api/memory/{id}` · `GET /api/memory/export` · `GET /api/tasks`

**Rails and catalogs**
`GET /api/worlds` (the two rosters) · `GET /api/greeting` · `GET /api/people` · `GET /api/live` · `GET /api/rails/background` · `GET /api/ambient` (SSE)

**Voice**
`GET /api/voice/status` · `WS /ws/voice` (16 kHz PCM in, 24 kHz PCM out, JSON control messages, camera frames in, tool calls out)

**Agentic Web Protocol**
`GET /.well-known/awp.json` · `GET /awp/manifest` · `POST /awp/a2a` (reaches the Mother; external callers are capped at Suggest) · `POST /awp/events/subscribe`

**Operations**
`GET /health` reports the phase, which integrations are live and whether persistence is on. `cargo run --bin migrate` applies migrations without booting the server.

## Voice and camera

Suzy runs on Gemini Live over a WebSocket the Rust server owns, following the adk-rust `realtime_voice` example: server-side voice activity detection with barge-in, input and output transcription, gapless PCM playback in the browser that flushes the moment you start speaking, and async tool handlers so Suzy can call `submit_intent` or read the session without stalling the stream. What you say fills the intent bar; what she says appears as captions.

With the camera on, frames go to the model about once a second and a local hand-gesture recogniser (MediaPipe, loaded from jsDelivr on first use, nothing leaves the browser) watches the same feed. Five gestures map to UI verbs, each confirmed with a ding: swipe left or right to change world, an open palm held still pauses the agents, a wave asks for today's briefing, a pinch closes the card in front. The classifier rules are pure JavaScript and unit-tested in Node.

## Testing

```bash
cargo test --lib                                   # unit tests, offline
cargo test --test validate -- awp_gate business_toml mock_deck combine_action keyword_router \
  artifact_paths session_store voice_state greeting_brand background_cards ambient_store \
  tour_prompts proactive_mock mcp_allowlist health_exposes domain_ intake_ mother_ ledger_ \
  permission_ gate_ pending_ memory_ effects_ consent_ tasks_ work_ home_ bus_ patterns_ baseline_ camera
cargo test --test validate -- postgres_schema ui_session_persists pg_agent_session   # needs DATABASE_URL
cargo test --test validate -- gemini_ router_agent suzy_agent                        # needs GOOGLE_API_KEY
cargo test --test validate deck_workflow_writes_three_artifacts -- --ignored         # full E2E with MCP + Gemini
node scripts/test-gesture-classifier.mjs                                             # gesture rules
cargo clippy --all-targets
```

Test filters are substrings, not regexes. CI runs the unit and offline groups against a Postgres service container, plus the Docker build, on every push and pull request.

## Project layout

| Path | What lives there |
|---|---|
| `src/main.rs`, `src/config.rs`, `src/state.rs` | Boot, environment, `AppState` and the session store |
| `src/mother/` | Intake, delegation, synthesis, agent bus, arbitration |
| `src/worlds/` | Work and Home mothers and agent registries |
| `src/agents/`, `src/orchestrator/` | Scenario workflows and card agents; the SSE streamer and per-scenario card shapes |
| `src/permissions/`, `src/memory/`, `src/intelligence/` | Gate, pending actions, audit; memory store and consents; ledger, patterns, baseline |
| `src/tools/` | MCP spawning, allowlist catalog and effect classes, the built-in tasks toolset |
| `src/voice/`, `src/routes/voice.rs` | Gemini Live runner, camera channel, the voice WebSocket |
| `src/routes/`, `business.toml` | Axum handlers and the AWP capability catalog |
| `web/` | The field UI and its scripts (see `CLAUDE.md` for the map) |
| `migrations/` | sqlx migrations, applied at boot |
| `tests/validate.rs` | The integration suite |
| `deploy/`, `Dockerfile` | Production image, Caddy TLS proxy, compose stack |

## Documentation

- [**Interactive one-page overview**](docs/personal-ai-os.html) — the concept and sprint plan as a single page (open locally; GitHub serves it as text)
- [Personal AI OS — concept and architecture](docs/PERSONAL_AI_OS.md) — vision, layers, Mother Agent, worlds, agent catalog, intelligence, baseline, balance, automation, memory, privacy, journeys, agent-to-agent protocol, API and data model appendices
- [Sprint plan](docs/SPRINT_PLAN.md) — Phase 2 as thirteen sprints and four releases, with task ids
- [Team progress](docs/PROGRESS.md) — who owns what, sprint by sprint, and what has landed
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md) — Phase 1 milestones and the R1 validation script
- [Specification](docs/SPECIFICATION.md) — Phase 1 requirements, SSE contract, AWP reference story
- [Architecture decisions](docs/adr/) — Mother Agent as the single orchestrator, domain model, permission modes and effect classes, content-free ledger, tenancy, prerequisites
- [`CLAUDE.md`](CLAUDE.md) — the operating manual for working in this repo: build, test, layout, and the seven rules every change must keep

## Team workflow

Four lanes, two parallel tracks, two-week sprints. Every task is a pull request to `main` behind a flag; shared files (`state.rs`, the SSE enum, `business.toml`, `mcp_allowlists.toml`, migrations) change through the Platform lane. Commit subjects follow `feat(scope): …`, `fix(scope): …`, `docs: …`, `ci: …` with task ids from the sprint plan, and no generated-by footers. Details in [`docs/PROGRESS.md`](docs/PROGRESS.md) and [`CLAUDE.md`](CLAUDE.md).

Agentrix OS is built by Zavora.
