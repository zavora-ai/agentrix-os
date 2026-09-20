# Agentrix OS

**The agentic operating system that works while you live.** One person, one AI OS, one Mother Agent, many specialised agents, one coordinated life.

[![CI](https://github.com/zavora-ai/agentrix-os/actions/workflows/ci.yml/badge.svg)](https://github.com/zavora-ai/agentrix-os/actions/workflows/ci.yml)
[![Built with Claude Fable 5.1](https://img.shields.io/badge/built%20with-Claude%20Fable%205.1-7c5cff)](https://www.anthropic.com/claude/fable)
[![Claude Fable 5.1 Build · Hackerhouse Africa](https://img.shields.io/badge/Claude%20Fable%205.1%20Build-Hackerhouse%20Africa%20%C2%B7%20Nairobi%2C%20Kenya-25d0c0)](#team-and-credits)

![Agentrix OS — the Work world with eight agents and live cards](docs/screenshots/02-work-world.jpg)

Agentrix OS is a personal AI operating system. You say what you want, by text or voice, and a **Mother Agent** works out which of sixteen specialised agents should act, runs them against your real tools, and answers with one voice. The agents are split into a **Work World** and a **Home World** that never see each other's data. Every action passes a **permission gate** you set per agent, and everything the system knows about you sits in a **memory you can read, correct, export and delete**.

It is a Rust/Axum server with a spatial browser UI, built on [adk-rust](https://github.com/zavora-ai/adk-rust), Gemini, the Model Context Protocol for tools, and the Agentic Web Protocol for other agents.

> Built at the **Claude Fable 5.1 Build**, Hackerhouse Africa, Nairobi, Kenya, by James, Kim, Robert and Jotham with **Claude Fable 5.1** as the engineering partner on every lane. See [Team and credits](#team-and-credits).

## Why an OS and not a chatbot

People run their lives through a dozen disconnected tools: work email, Slack, two calendars, banking, health trackers, family chats, news, music. Each has its own notifications and its own "assistant", and none knows what the others are doing. The person is the only integration layer.

A chatbot answers questions. An operating system **owns the context**, **schedules work across many processes**, and **enforces permissions**. Agentrix OS applies those three ideas to a person's digital life:

| OS concept | Agentrix OS equivalent |
|---|---|
| Kernel and scheduler | The Mother Agent: routes, delegates, arbitrates, synthesises |
| Processes | Specialised agents, one per life area, each with narrow tools |
| Partitions | Work World and Home World, isolated by default, reconciled only by the kernel |
| Telemetry | A content-free activity ledger feeding an intelligence layer |
| Capabilities | Observe, Suggest or Automate, per agent, per tool effect |
| A filesystem the user owns | Personal memory: known, assumed, recommended |

Ten principles guide every decision, from "orchestrate, don't chat" to "health and money stay conservative". They are listed with their reasoning in [§1.3 of the concept](docs/PERSONAL_AI_OS.md#13-design-principles). One of them shapes everything you will see when you run it: **honest data**. Without a Gemini key the scenarios stream scripted mocks and say so; without a tool server an agent ships as a labeled stub; nothing is ever fabricated to look connected.

## What it does

<table>
<tr>
<td width="50%"><img src="docs/screenshots/01-greeting.jpg" alt="Greeting screen"><br><sub><b>Greets you from real facts.</b> Suzy, the voice of the Mother Agent, composes the opening line only from connected integrations. With none connected, she says so.</sub></td>
<td width="50%"><img src="docs/screenshots/02-work-world.jpg" alt="Work world"><br><sub><b>Runs a Work World.</b> Eight work agents with their authority modes; cards bloom in the field as they finish; labeled stubs mark integrations that do not exist yet.</sub></td>
</tr>
<tr>
<td><img src="docs/screenshots/03-home-world.jpg" alt="Home world"><br><sub><b>Keeps Home apart.</b> Swipe, scroll, arrow keys or the pager. Theme, roster and visible cards change with the world; only the Mother sees both.</sub></td>
<td><img src="docs/screenshots/04-mother-chat.jpg" alt="Mother chat"><br><sub><b>Remembers with provenance.</b> "Remember I live in Nairobi" becomes a <i>known</i> item, cited later as "(you told me)". Inferred items are <i>assumed</i> and can be confirmed, corrected or forgotten.</sub></td>
</tr>
<tr>
<td><img src="docs/screenshots/05-deck-artifacts.jpg" alt="Deck workflow"><br><sub><b>Does real work.</b> "Build me a pitch deck" runs Excel, Word and PowerPoint agents against real tool servers and links three downloadable files.</sub></td>
<td><img src="docs/screenshots/06-approvals.jpg" alt="Approvals inbox"><br><sub><b>Asks before acting.</b> Anything an agent may not do on its own waits in the approvals inbox. Approve, edit or reject, singly or in batch; every decision is audited.</sub></td>
</tr>
</table>

You can also **talk to it**. Suzy runs on Gemini Live with barge-in and captions, and with the camera on she reads five hand gestures: swipe to change world, an open palm to pause the agents, a wave for today's briefing, a pinch to close a card. Each is confirmed with a ding.

## How it works

![Architecture — surfaces, Mother Agent, two worlds, intelligence, memory, automation and trust, privacy, integrations](docs/architecture.svg)

An intent enters at the top and travels down. Each row links to the concept section that specifies it.

| Layer | What it does | Code |
|---|---|---|
| **Surfaces** | The spatial field (cards, drag-to-fuse, fling-to-dismiss, worlds pager), the Mother chat panel, the approvals inbox, live voice and camera, and the Agentic Web Protocol endpoint for external agents | `web/`, `src/routes/`, `src/voice/` |
| **Mother Agent** ([§3](docs/PERSONAL_AI_OS.md#3-mother-agent)) | Intake, context assembly, delegation, arbitration, synthesis, action gate. It has no tools of its own that reach the outside world, so a wrong routing decision can never send an email | `src/mother/` |
| **Worlds** ([§4](docs/PERSONAL_AI_OS.md#4-work-world), [§5](docs/PERSONAL_AI_OS.md#5-home-world)) | A Work Mother over Productivity, Email, Team Comms, Project, Research & Knowledge, Work Automation, Career, Professional Social. A Home Mother over Family, Personal Productivity, Health & Wellness, Finance (read-only), Entertainment, Social & Fun, Travel, Personal Social | `src/worlds/` |
| **Agent contract** ([§6](docs/PERSONAL_AI_OS.md#6-specialized-agents)) | Every agent declares its world, mission, tool allowlist with an effect class per tool, default authority mode and memory scope | `mcp_allowlists.toml`, `src/tools/allowlist.rs` |
| **Intelligence** ([§7](docs/PERSONAL_AI_OS.md#7-intelligence-layer), [§8](docs/PERSONAL_AI_OS.md#8-personal-baseline--going-off-the-book)) | A content-free ledger feeds deterministic pattern aggregation and a personal baseline (median and MAD, 14-day warm-up, drift detection). The model only phrases observations, in neutral language | `src/intelligence/` |
| **Memory** ([§11](docs/PERSONAL_AI_OS.md#11-memory-architecture)) | Known, assumed and recommended items, domain-scoped, with provenance, consent-linked, encrypted when sensitive | `src/memory/` |
| **Automation and trust** ([§10](docs/PERSONAL_AI_OS.md#10-automation-layer)) | A gate wraps every toolset and decides by the tool's effect class and the agent's mode; pending actions, audit, undo | `src/permissions/` |
| **Privacy and security** ([§12](docs/PERSONAL_AI_OS.md#12-privacy--security)) | JWT and Google OAuth, per-agent allowlists, domain isolation, consents, encryption at rest, content-free logs | `src/auth.rs`, `src/awp_gate.rs` |
| **Integrations** | Fourteen MCP tool servers spawned as child processes, plus AWP peers | `src/tools/mcp.rs` |

The permission gate is the part to understand first. Each agent runs in one of three modes, and each tool carries an effect class; the gate decides at the tool boundary:

| Effect class | Examples | Observe | Suggest | Automate |
|---|---|---|---|---|
| `read` | list inbox, today's events | runs | runs | runs |
| `write_local` | create a draft, save a deck, create a task | denied | runs | runs |
| `schedule_with_others`, `send_external`, `delete` | create an event with attendees, send a reply, archive | denied | **pending action** | only through an approved recipe |
| `publish_public`, `financial` | post publicly, move money | denied | pending, with confirmation | **never** |

## Getting started

### Prerequisites

- Rust 1.95 or newer.
- A sibling checkout of [adk-rust](https://github.com/zavora-ai/adk-rust) on `main` at `../adk-rust`; `Cargo.toml` uses path dependencies.
- Optional, for real tools: the [mcp-servers](https://github.com/zavora-ai/mcp-servers) monorepo at `../mcp-servers`.
- Optional, for live agents and voice: a Gemini API key. Optional, for persistence and sign-in: Docker.

### Run it

```bash
git clone https://github.com/zavora-ai/agentrix-os.git
git clone https://github.com/zavora-ai/adk-rust.git      # sibling checkout
cd agentrix-os
cp .env.example .env
cargo run
open http://127.0.0.1:9847
```

Without a key you get the scripted tour: the same UI, mock data, labelled as simulated. Add `?demo=1` to the URL for the fully offline version.

### Try these first

1. Type **What's happening with work?** in the intent bar. The Mother fans out to the Work World; cards bloom; Suzy summarises.
2. Click **Home ›** in the pager, or swipe the background. The roster on the right changes to the eight home agents and the theme turns green.
3. Open the chat with the ✦ button and type **Remember I live in Nairobi**. Then ask **What do I need to know today?** and watch the citation "(you told me)".
4. Type **Build me a pitch deck**. With the document servers built (below), three real files appear; say **combine** to merge them.
5. Drag a card onto another to fuse them; fling a card off the field to snooze its agent.

### Go live, step by step

**1. Real agents.** Put your key in `.env` as `GOOGLE_API_KEY`. The Mother's intake and synthesis, the greeting and Suzy's voice now run on the model. One trap: dotenv never overrides a variable your shell already exports, so a stale key in `~/.zshrc` wins over `.env`.

**2. Real tools.** Build the servers you want from `../mcp-servers`; each compiles on its own in about half a minute. The app finds them at the default paths in `.env.example`.

```bash
for s in worksheet-mcp docx-mcp mcp-slides mcp-news mcp-weather; do (cd ../mcp-servers/$s && cargo build --release); done
```

| Server | Powers | Credentials |
|---|---|---|
| worksheet, docx, slides | the deck workflow, Work Automation | none |
| news, weather, maps, market data | morning brief, live headlines, Lisbon, entertainment | none; GNews headlines need `GNEWS_API_KEY` |
| calendar | Productivity, the Today card | `GOOGLE_CALENDAR_TOKEN` or `mcp-calendar auth google` |
| email | the Email agent, the Needs-you card | Gmail OAuth via `mcp-email auth gmail`, or SMTP and IMAP |
| Slack, CRM, GitHub | Team Comms, Project | `SLACK_BOT_TOKEN`, a CRM token, `GITHUB_TOKEN` |
| banking, real estate | Finance (read-only), Travel | Plaid or Mono; Attom or Bayut |

A server that cannot start leaves its agent as a labeled stub, never a fake card.

**3. Persistence and sign-in.** Start Postgres and set `DATABASE_URL` and `JWT_SECRET`. Migrations run at boot. This unlocks the approvals inbox, memory across restarts, and the trust routes; without it everything is in-memory and anonymous ([ADR-006](docs/adr/006-phase2-requires-signed-in-user-and-postgres.md)).

```bash
docker compose up -d        # Postgres on port 5434
```

**4. Voice and camera.** With a key set, the 🎤 button opens a Gemini Live session: what you say fills the intent bar, what Suzy says plays back gaplessly and stops the moment you interrupt. The 📷 button adds the camera; a local recogniser (MediaPipe, downloaded on first use, frames never leave the browser) turns your gestures into UI verbs with a ding.

## Configuration

Every setting is an environment variable, read in `src/config.rs` and documented in `.env.example`.

| Variable | Purpose | Default |
|---|---|---|
| `GOOGLE_API_KEY` | Gemini for agents, the Mother and live voice; unset means scripted mocks | unset |
| `GEMINI_MODEL` | Text model for agents | `gemini-3.1-flash-lite` |
| `GEMINI_LIVE_MODEL`, `VOICE_NAME` | Live voice model and voice | `models/gemini-3.8-live`, `Aoede` |
| `AGENTRIX_CAMERA` | Camera channel over the voice socket | `1` |
| `MCP_*_PATH` | Binaries for worksheet, docx, slides, calendar, email, news, weather, market data, Slack, CRM, banking, GitHub, maps, real estate | `../mcp-servers/<crate>/target/release/<bin>` |
| `DATABASE_URL`, `JWT_SECRET` | Postgres persistence and sign-in; both required for a real deployment | unset |
| `GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET` | Sign in with Google; without them, use the dev sign-in | unset |
| `MEMORY_MASTER_KEY`, `LEDGER_HASH_KEY` | Encrypt sensitive memory values; hash ledger subjects | dev keys, with a boot warning |
| `AGENTRIX_WORK_MOTHER`, `AGENTRIX_HOME_MOTHER` | `0` falls back to direct per-agent fan-out | on |
| `AGENTRIX_ALLOW_DEMO`, `BASE_URL`, `AGENTRIX_SIGNUP_ENDPOINT` | Demo mode off localhost, public URL, early-access form | see `.env.example` |

## API

Every public route has a `[[capabilities]]` entry in `business.toml` with an access level, enforced by the AWP gate. `anonymous` routes serve the tour and static catalogs; anything that acts, changes settings or reads personal data is `known`, which means signed in.

| Concern | Routes |
|---|---|
| Sessions and intents | `POST /api/sessions` · `POST /api/sessions/{id}/intent` (SSE) · `POST /api/sessions/{id}/chat` (SSE through the Mother; `GET` returns the transcript) · `POST /api/sessions/{id}/action` · `POST /api/sessions/{id}/events` |
| Field state | `GET /api/sessions/{id}/cards` · `GET /api/sessions/{id}/agents` · `POST /api/agents/{id}/snooze` and `wake` · `POST /api/sessions/{id}/commit` · `POST /api/sessions/{id}/fuse` |
| Trust | `GET /api/actions` · `POST /api/actions/{id}/approve`, `reject`, `edit` · `POST /api/actions/approve` (batch) · `GET /api/audit` · `GET`/`PUT /api/permissions` · `POST /api/pause` · `POST /api/resume` · `GET`/`PUT /api/consents` |
| Memory and tasks | `GET`/`POST`/`DELETE /api/memory` · `PATCH`/`DELETE /api/memory/{id}` · `GET /api/memory/export` · `GET /api/tasks` |
| Rails and catalogs | `GET /api/worlds` · `GET /api/greeting` · `GET /api/people` · `GET /api/live` · `GET /api/rails/background` · `GET /api/ambient` (SSE) |
| Voice | `GET /api/voice/status` · `WS /ws/voice` — 16 kHz PCM in, 24 kHz PCM out, JSON control messages, camera frames in, tool calls out |
| Agentic Web Protocol | `GET /.well-known/awp.json` · `GET /awp/manifest` · `POST /awp/a2a` (reaches the Mother; external callers are capped at Suggest) · `POST /awp/events/subscribe` |
| Operations | `GET /health` — phase, live integrations, persistence · `cargo run --bin migrate` applies migrations without booting |

The SSE event contract is `FieldEvent` in `src/events/sse.rs`, mirrored by TypeScript types in `web/static/types/`.

## Developing

Read [`CLAUDE.md`](CLAUDE.md) first. It is the operating manual: build, test, layout, and the seven rules every change must keep, such as honest data, least privilege, and the shared SSE contract.

### Layout

| Path | What lives there |
|---|---|
| `src/main.rs`, `src/config.rs`, `src/state.rs` | Boot, environment, `AppState` and the session store |
| `src/mother/` | Intake, delegation, synthesis, agent bus, arbitration |
| `src/worlds/` | Work and Home mothers and agent registries |
| `src/agents/`, `src/orchestrator/` | Scenario workflows and card agents; the SSE streamer |
| `src/permissions/`, `src/memory/`, `src/intelligence/` | Gate, pending actions, audit; memory store and consents; ledger, patterns, baseline |
| `src/tools/` | MCP spawning, allowlist catalog and effect classes, the built-in tasks toolset |
| `src/voice/`, `src/routes/voice.rs` | Gemini Live runner, camera channel, the voice WebSocket |
| `src/routes/`, `business.toml` | Axum handlers and the AWP capability catalog |
| `web/` | The field UI and its scripts |
| `migrations/` | sqlx migrations, applied at boot |
| `tests/validate.rs` | The integration suite |
| `deploy/`, `Dockerfile` | Production image, Caddy TLS proxy, compose stack |

### Tests

```bash
cargo test --lib                                   # unit tests, offline
cargo test --test validate -- awp_gate business_toml mock_deck combine_action keyword_router \
  artifact_paths session_store voice_state greeting_brand background_cards ambient_store \
  tour_prompts proactive_mock mcp_allowlist health_exposes domain_ intake_ mother_ ledger_ \
  permission_ gate_ pending_ memory_ effects_ consent_ tasks_ work_ home_ bus_ patterns_ baseline_ camera
cargo test --test validate -- postgres_schema ui_session_persists pg_agent_session   # needs DATABASE_URL
cargo test --test validate -- gemini_ router_agent suzy_agent                        # needs GOOGLE_API_KEY
cargo test --test validate deck_workflow_writes_three_artifacts -- --ignored         # full E2E with MCP and Gemini
node scripts/test-gesture-classifier.mjs                                             # camera gesture rules
cargo clippy --all-targets
```

Test filters are substrings, not regexes. CI runs the unit and offline groups against a Postgres service container, plus the Docker build, on every push and pull request.

### Workflow

Every task is a pull request to `main` behind a flag, using the task ids from the sprint plan. Shared files (`state.rs`, the SSE enum, `business.toml`, `mcp_allowlists.toml`, migrations) change through the Platform lane. Commit subjects follow `feat(scope): …`, `fix(scope): …`, `docs: …`, `ci: …`, with a body that says why. Details in [`docs/PROGRESS.md`](docs/PROGRESS.md).

## Documentation

- [Interactive one-page overview](docs/personal-ai-os.html) — the concept and sprint plan as one page; open locally, GitHub serves it as text
- [Personal AI OS: concept and architecture](docs/PERSONAL_AI_OS.md) — vision, layers, Mother Agent, worlds, agent catalog, intelligence, baseline, balance, automation, memory, privacy, user journeys, agent-to-agent protocol, API and data model appendices
- [Sprint plan](docs/SPRINT_PLAN.md) — Phase 2 as thirteen sprints and four releases
- [Team progress](docs/PROGRESS.md) — who owns what, sprint by sprint
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md) — Phase 1 milestones and the Release 1 validation script
- [Specification](docs/SPECIFICATION.md) — Phase 1 requirements, SSE contract, AWP reference story
- [Architecture decisions](docs/adr/) — single orchestrator, domain model, permission modes and effect classes, content-free ledger, tenancy, prerequisites
- [`CLAUDE.md`](CLAUDE.md) — the operating manual for working in this repo

## Team and credits

Agentrix OS was built at the **Claude Fable 5.1 Build** at **Hackerhouse Africa, Nairobi, Kenya**, with **Claude Fable 5.1** as the engineering partner on every lane through Claude Code: from the Phase 2 concept and sprint plan to the Rust services, the browser UI, the tests, and the verification runs recorded in each pull request. The product's own text agents are moving onto Claude Fable 5.1 with Gemini as the fallback ([#19](https://github.com/zavora-ai/agentrix-os/pull/19)).

Four people, four lanes, one AI pair. Areas are what each has landed on `main` or has in review.

| Contributor | Lane | What they built |
|---|---|---|
| **James Karanja Maina** · [@jkmaina](https://github.com/jkmaina) | Tech lead, product owner · **Platform and data** | Phase 1 end to end: the live SSE shell, the real deck workflow over MCP, the seven scenarios, ambient agents on cron, Postgres sessions and JWT auth, Gemini Live voice, the Docker and Caddy deploy. Phase 2: the team plan and ADRs 005 and 006, the tasks store and Productivity tools, persisted consents, migrations, a CI that actually runs, the runner-session fix that made the Mother's LLM paths work, the voice fix from the mia pattern, the Agentrix rebrand, the world rosters, local camera gestures, drag and swipe fixes, this README. |
| **Kim** · [@swiftkimani](https://github.com/swiftkimani) | **Agents and worlds** | The Personal AI OS concept, the sprint plan and the one-page overview; `CLAUDE.md`; Phase 2 Release 1: Mother Agent intake, delegation and synthesis, the content-free activity ledger, the permission gate with pending actions and audit, personal memory with provenance and encryption; the Work World and Home World mothers and agent registries, follow-up tracker, health guardrail and labeled stubs; the agent bus and arbitration v1; the camera channel that lets Suzy see. |
| **Robert Kimaiyo** · [@Robertkip](https://github.com/Robertkip) | **Surface** | The two themed worlds with swipe navigation and the domain lens, mode badges on cards and tiles, the Mother chat panel, the approvals inbox with live expiry and the away feed, content-free UI signals for the ledger, TypeScript types for the SSE contract. |
| **Jotham Siror** · [@Jotham-Siror](https://github.com/Jotham-Siror) | **Intelligence** | Daily pattern aggregation over the ledger and the personal baseline (median and MAD, weekday and weekend classes, 14-day warm-up, drift detection) with the synthetic six-week drift fixture; running the text agents on Claude Fable 5.1 with Gemini fallback (in review). |
| **Claude Fable 5.1** · [Anthropic](https://www.anthropic.com/claude/fable) | **AI pair on every lane** | Concept and plan review, ADR drafting, Rust and JavaScript implementation, test suites, headless browser verification, live agent and MCP verification, and the pull request write-ups, driven by the team through Claude Code at the Hackerhouse Africa build. |

Agentrix OS is a product of Zavora.
