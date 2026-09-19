# Agentrix OS (spatial-os) — Product Specification

**Status:** Draft v1.2  
**Date:** 2026-06-22  
**Owner:** Zavora Technologies  
**Task plan:** [`IMPLEMENTATION_PLAN.md`](./IMPLEMENTATION_PLAN.md) — tickable checklist per milestone.  
**Reference stack:** [adk-rust](https://github.com/zavora-ai/adk-rust), [adk-awp](https://github.com/zavora-ai/adk-rust/tree/main/adk-awp) / [AWP](https://agenticwebprotocol.com), excel-agent-app, docx-agent-app, mia, `mcp-servers/mcp_slides` (slides-mcp-server)

---

## 1. Purpose of this document

This specification defines the **complete target system** for Agentrix OS: requirements, design, APIs, agent topology, and a phased delivery plan. It is written **without architectural tradeoffs** — every concept visible in `field.html` must eventually be backed by real agents, tools, and persistence.

Work ships in **agile milestones** (vertical slices). Each milestone ends with **git commits**, a **user validation script**, and a decision gate before the next milestone starts. Milestones defer *timing*, not *scope*.

### 1.1 Guiding constraints

| Constraint | Meaning |
|------------|---------|
| **Preserve the design** | `field.html` visual language, layout tiers, VerbUI gestures, greeting flow, and Suzy persona remain the product surface. |
| **Actualize the backend** | Simulated timers, static `scenarios{}` data, and scripted Conducting are replaced by server-driven events and real agent execution. |
| **No scope cuts** | Mock integrations are allowed only as *temporary stubs* with explicit replacement tasks; they are not permanent substitutes. |
| **Validate every milestone** | A human runs a scripted demo and signs off before merge to `main` for that milestone. |
| **AWP as agent surface** | The same Axum server serves humans (`field.html`) and external agents ([AWP](https://agenticwebprotocol.com)) — no second app, no duplicate orchestration. |

---

## 2. Product vision

**Agentrix OS** is an agentic operating system where the user expresses intent in natural language (text or voice) and the system:

1. **Blooms** work into spatial cards — each card is a sub-agent with visible progress.
2. **Orchestrates** parallel and dependent agent work (e.g. slides wait for numbers + narrative).
3. **Summarizes** via Suzy (coordinator persona) when work settles.
4. **Acts** on a second turn — action verbs trigger Conducting (gesture + real execution).
5. **Runs proactively** in the background (research, scouting, making) without user initiation.
6. **Surfaces** people, live feeds, and ambient context in persistent shell regions.

**Positioning:** *"The OS that works while you live."*

### 2.1 Dual role: product + AWP reference implementation

Agentrix OS is intentionally the **canonical demonstration** of the [Agentic Web Protocol (AWP)](https://agenticwebprotocol.com):

| Surface | Consumer | Entry |
|---------|----------|-------|
| **Human** | Browser | `field.html` — spatial cards, VerbUI, voice |
| **Agent** | External AI agents | `/.well-known/awp.json` → manifest → `/awp/a2a` |

Both surfaces hit the **same** session, intent, and agent orchestration layer. AWP does not add a parallel stack — it is `awp_routes(state).merge(app)` on the existing Axum server (pattern from `examples/awp_agent`).

**Why this app is a strong AWP demo:**

- Declares **machine-readable capabilities** (submit intent, run scenario, fuse cards, subscribe to proactive events) that mirror what the UI already does.
- **`business.toml`** encodes Zavora brand voice (Suzy), policies, and scenario catalog — agents discover intent without scraping HTML.
- **Proactive agents** naturally map to AWP **event subscriptions** (webhook when scout finds a price drop).
- **Trust levels** gate sensitive capabilities (email draft, banking) without bespoke auth per integration.
- Proves AWP’s core thesis: *one service, humans and agents, same capabilities.*

---

## 3. Current state (baseline)

### 3.1 What exists

| Asset | State |
|-------|-------|
| `field.html` | ~1,877 lines — full UI, 7 scenarios, VerbUI, greeting, signup |
| `audio/*.wav` + `gen_audio.py` | Prerecorded Suzy voice clips (Gemini TTS) |
| `capture.js` | Marketing demo frame capture |
| `Cargo.toml` | adk-rust path dependency declared |
| `src/` | **Does not exist** — no backend |

### 3.2 What is simulated today

- Scenario selection (`pickScenario`) — keyword rules, no LLM.
- Card streaming (`runCard`) — `setInterval` over static `stream[]` strings.
- Card resolution — static `resolve{}` payloads.
- Auto-surfaces (excel/docs/slides) — CSS animations only.
- Fuse / combine — local DOM updates + prerecorded `fuse.wav`.
- Agent rails — in-memory `Map`, lost on refresh.
- People / Live rails — hardcoded arrays.
- Proactive agents — rotating placeholder text.
- Conducting — scripted `conductFuse()` with fixed card title pairs.
- Greeting content — fixed copy + prerecorded clip (not personalized).

### 3.3 Baseline commit reference

Latest UI milestone: greeting flow, 7-scenario tour, early-access signup, LinkedIn conversion tracking.

---

## 4. Target state (complete system)

### 4.1 Logical architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Browser — field.html (preserved design)                                 │
│  ┌─────────────┐ ┌──────────────┐ ┌─────────────┐ ┌──────────────────┐ │
│  │ Greeting    │ │ Card field   │ │ Shell rails │ │ Intent + voice   │ │
│  │ (personal)  │ │ + VerbUI     │ │ People/Live │ │ bar              │ │
│  │             │ │ + Auto-*     │ │ Agents      │ │                  │ │
│  └──────┬──────┘ └──────┬───────┘ └──────┬──────┘ └────────┬─────────┘ │
│         │               │                │                  │           │
│         └───────────────┴────────────────┴──────────────────┘           │
│                                    │ HTTP / SSE / WS                     │
└────────────────────────────────────┼────────────────────────────────────┘
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  spatial-os server (Rust / Axum)                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ Session API  │  │ Intent API   │  │ Agent rail   │  │ Ambient     │ │
│  │ + auth       │  │ (SSE stream) │  │ state API    │  │ event bus   │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬──────┘ │
│         │                 │                 │                  │        │
│         └─────────────────┴─────────────────┴──────────────────┘        │
│                                    │                                     │
│                    ┌───────────────▼───────────────┐                     │
│                    │  Suzy (coordinator LlmAgent)  │                     │
│                    │  + LlmConditionalAgent router │                     │
│                    └───────────────┬───────────────┘                     │
│         ┌──────────────────────────┼──────────────────────────┐         │
│         ▼                          ▼                          ▼         │
│  ┌─────────────┐           ┌─────────────┐           ┌─────────────┐     │
│  │ Scenario    │           │ Scenario    │           │ Ambient     │     │
│  │ workflows   │           │ sub-agents  │           │ agents      │     │
│  │ (Parallel/  │           │ (per card)  │           │ (Cron/      │     │
│  │  Sequential)│           │             │           │  Webhook)   │     │
│  └──────┬──────┘           └──────┬──────┘           └──────┬──────┘     │
│         │                         │                         │            │
│         └─────────────────────────┴─────────────────────────┘            │
│                                    │ adk-runner                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  AWP layer (adk-awp) — same process, no duplicate logic           │   │
│  │  /.well-known/awp.json · /awp/manifest · /awp/a2a · /awp/events* │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────┼────────────────────────────────────┘
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Tools & integrations                                                    │
│  worksheet-mcp · docx-mcp · slides-mcp-server · mcp-calendar ·          │
│  mcp-email · mcp-news · mcp-slack · mcp-crm · mcp-banking · mcp-github  │
│  mcp-maps · mcp-weather · mcp-real-estate · computer-use-mcp · memory    │
└─────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Core runtime concepts

| Concept | Definition | Persistence |
|---------|------------|-------------|
| **Session** | One user visit; holds agent state, artifacts, rail positions | Postgres (prod) / memory (dev) |
| **Intent** | User utterance (text or transcribed voice) | Logged per session |
| **Scenario** | Routed workflow (`morning`, `deck`, `lisbon`, …) | Derived per intent |
| **Card** | UI surface bound to one sub-agent run | `app:cards:{id}` in session state |
| **Artifact** | File or structured output (xlsx, docx, deck, draft email) | `artifacts/{user}/{session}/` |
| **Action turn** | Second intent matching action verbs (`combine`, `handle it`, …) | Triggers Conducting + backend execution |
| **Ambient job** | Background agent run (proactive rail) | `app:ambient:{agent_id}` |

---

## 5. Functional requirements

Requirements are numbered **FR-###**. All are **mandatory** for v1.0 GA; milestones only control delivery order.

### 5.1 Input & greeting

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-001 | User can start from greeting screen ("Start my day" / ambient wallpaper path). | Both paths reach a live scenario. |
| FR-002 | Greeting voice is personalized from real calendar/email summary when integrations exist; falls back to generic clip offline. | Morning scenario data reflected in greeting copy + audio regeneration path. |
| FR-003 | Intent bar accepts text input and submits on Enter / ↑ button. | Same as today. |
| FR-004 | Voice input via Web Speech API transcribes to intent bar and submits. | Chrome/Safari; graceful message elsewhere. |
| FR-005 | Quick chips submit canned intents. | Chips remain; may be server-configurable later. |

### 5.2 Scenario routing & card bloom

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-010 | System routes intent to one of seven scenarios: `morning`, `lisbon`, `week`, `deck`, `people`, `live`, `proactive`. | Routing via `LlmConditionalAgent` or equivalent; keyword fallback in dev only. |
| FR-011 | Scenario spawns N cards (typically 3) with staggered bloom animation driven by server `card_spawn` events. | Visual parity with current `launch()`. |
| FR-012 | Each card shows agent id, working → done status, streaming status lines from real tool/LLM events. | No `setInterval` fake streams in production mode. |
| FR-013 | Cards with `waitsFor` do not resolve until dependency cards complete server-side. | `deck` slides after excel+docs; `morning` brief after calendar+inbox. |
| FR-014 | Attention cards (`attention: true`) show "Act now" pill when server flags urgency. | Server sets `attention` from tool output. |
| FR-015 | Origin chip shows the user's triggering intent text. | Preserved. |

### 5.3 VerbUI & Conducting

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-020 | User can focus a card (tap / coverflow on desktop). | Unchanged interaction. |
| FR-021 | User can drag card A onto card B to **fuse** — server recomputes target using source artifact/context. | `POST /api/fuse` returns updated card body + artifact refs. |
| FR-022 | User can **fling** a card to Resting rail — snoozes agent server-side. | `POST /api/agents/{id}/snooze`; survives refresh. |
| FR-023 | User can click primary action → **commit** — server executes action (book, save, draft, …) then minimizes card. | 3s working state may shorten if server responds faster; outcome is real. |
| FR-024 | Second-turn action intents (`combine`, `handle it`, `book it`, …) trigger **Conducting** — UI gesture + server orchestration. | Hand animation preserved; backend performs real combine/route. |
| FR-025 | Deck `combine` produces a single merged artifact (pinned card, not snoozed). | Downloadable deck file; `deck_done` audio or live TTS. |
| FR-026 | Help card appears when all cards are dismissed/handled. | Preserved. |

### 5.4 Suzy coordinator

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-030 | When all cards in a scenario resolve, Suzy shows HTML summary of outcomes. | Generated by coordinator agent from session state, not static `suzySummaries{}`. |
| FR-031 | Suzy summary is spoken (prerecorded clip match or live TTS/WebSocket). | Phase: clip hash match → live voice in final milestone. |
| FR-032 | After summary, system arms guided suggestion for next action/scenario. | Tour order preserved; suggestions server-aware. |
| FR-033 | Suzy bubble reacts to audio amplitude when served over HTTP (not `file://`). | Analyser path works on deployed URL. |

### 5.5 Shell regions

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-040 | **People rail** (work + family) shows live presence from integrations. | Google/Slack/Contacts or staged adapter; no hardcoded `people{}`. |
| FR-041 | **Live feed** carousel shows real headlines/posts from configured sources. | `mcp-news` (`gnews_top_headlines`, `hn_stories`, etc.); swipe preserved. |
| FR-042 | **Active agents** rail lists running sub-agents with spinner → done glyph. | Synced from session state + SSE. |
| FR-043 | **Resting** rail lists snoozed agents; click wakes and re-activates. | Persisted; wake requeues if needed. |
| FR-044 | **Proactive** rail lists background agents with current task description. | Driven by ambient agent events. |
| FR-045 | **Background cards** flank field; click submits mapped intent. | Intent mapping configurable server-side. |

### 5.6 Auto-surfaces (artifact cards)

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-050 | `surface: excel` cards render live preview of workbook being built. | SSE streams cell/sheet events; embed SheetJS preview (from excel-agent-app). |
| FR-051 | `surface: docs` cards render document preview. | docx preview or HTML render of content. |
| FR-052 | `surface: slides` cards render slide filmstrip updating as slides are created. | Driven by `slides-mcp-server` events (`add_slide`, `describe_presentation`); optional `render_slide` PNG thumbnails for filmstrip frames. |
| FR-053 | Resolved Auto-* cards expose Save/Open actions that reference real artifact URLs. | Files exist on server (.xlsx, .docx, .pptx). |
| FR-054 | **Auto-Slides** sub-agent uses `slides-mcp-server` exclusively — not docx-mcp or ad-hoc HTML. | `auto-slides` card produces a real `.pptx` via `create_presentation` / `add_slide` / `save_presentation`. |
| FR-055 | Deck **combine** merges excel + docs context into the Auto-Slides `.pptx` (charts/tables from xlsx, narrative from docx). | `combine_agent` calls slides-mcp tools after reading sibling artifacts; pinned card serves final `.pptx`. |

### 5.7 Scenarios (full agent definitions)

Each scenario MUST reach the richness of the current demo, with real data sources.

| Scenario | Cards | MCP servers (v1.0 target) |
|----------|-------|---------------------------|
| **morning** | Today, Needs you, Brief | `mcp-calendar`, `mcp-email`, `mcp-news`, `mcp-weather` |
| **lisbon** | Flights, Stay, Itinerary | `mcp-maps`, `mcp-weather`, `mcp-real-estate` + **travel gap** (see Appendix C) |
| **week** | Money, Health, Focus | `mcp-banking`, `mcp-github` + **health gap** (no wearable MCP yet) |
| **deck** | Auto-Excel, Auto-Docs, Auto-Slides | `worksheet-mcp`, `docx-mcp`, `slides-mcp-server` |
| **people** | Team, Priya, Connections | `mcp-slack`, `mcp-crm`, `mcp-calendar` |
| **live** | Headlines, Markets, Now | `mcp-news` (+ `mcp-market-data` for portfolio depth) |
| **proactive** | Research, Scout, Maker | `mcp-news`, `mcp-real-estate`, `mcp-search` (+ `media-mcp` for Maker) |

| ID | Requirement |
|----|-------------|
| FR-060 | All seven scenarios implemented with real agent workflows — no permanent static JSON. |
| FR-061 | Each sub-agent is an `LlmAgent` (or `CustomAgent` for deterministic adapters) with scoped tools. |
| FR-062 | Scenario orchestration uses `ParallelAgent` / `SequentialAgent` matching `waitsFor` dependencies. |

### 5.8 Proactive / ambient layer

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-070 | Background agents run on schedules (`CronTrigger`) or external events (`WebhookTrigger`). | adk-agent `AmbientAgent` |
| FR-071 | Completed ambient work surfaces as proactive scenario cards or rail tile updates. | User can open "Show me what you found". |
| FR-072 | Ambient agents respect snooze / user focus (do not bloom cards during DND). | `app:user:dnd` state flag. |

### 5.9 Identity, sessions, marketing

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-080 | Early-access signup (Formspree + LinkedIn conversion) remains functional. | Unchanged marketing flow. |
| FR-081 | Authenticated users have persistent sessions across devices. | JWT or cookie auth. |
| FR-082 | UTM/referrer captured on signup preserved. | Unchanged. |

### 5.10 Agentic Web Protocol (AWP)

AWP is a **thin protocol layer** on the existing server. Requirements map 1:1 to `adk-awp` + `business.toml`; no separate agent runtime.

| ID | Requirement | Acceptance |
|----|-------------|------------|
| FR-083 | Server exposes all 7 standard AWP endpoints via `awp_routes()`. | Conformance checks in `adk-awp/tests/conformance_tests.rs` pass against deployed URL. |
| FR-084 | `business.toml` describes Agentrix OS: site identity, Suzy `brand_voice`, policies, and `[[capabilities]]` for each public `/api/*` action. | `GET /awp/manifest` lists capabilities that resolve to real routes. |
| FR-085 | `POST /awp/a2a` dispatches to the same intent/action pipeline as the human UI (create session → submit intent → SSE or aggregated response). | External agent "Build me a pitch deck" produces same artifacts as browser. |
| FR-086 | Proactive completions are publishable via `POST /awp/events/subscribe` (HMAC-signed webhooks). | Scout/research completion triggers webhook to subscriber. |
| FR-087 | Sensitive capabilities (email send, banking, CRM write) declare `access_level` ≥ `known` in `business.toml`; enforced by AWP trust middleware. | Anonymous request to gated capability returns AWP error envelope. |

---

## 6. Non-functional requirements

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-001 | First card visible | < 3s p95 after intent submit (excl. LLM cold start) |
| NFR-002 | Status line updates | SSE latency < 500ms from tool event |
| NFR-003 | MCP spawn | worksheet-mcp, docx-mcp, and slides-mcp-server warm at server boot |
| NFR-004 | Availability | 99.5% for hosted prototype |
| NFR-005 | Security | Auth on `/api/*`; artifact URLs scoped to user/session |
| NFR-006 | Observability | Structured tracing per card/agent (`tracing` + request id) |
| NFR-007 | Cost | Prerecorded audio when text matches known clip; live TTS only on novel summaries |
| NFR-008 | Offline demo | `?demo=1` or `localStorage` flag runs legacy simulated mode for marketing capture |
| NFR-009 | Accessibility | Intent input ≥16px on mobile; keyboard coverflow (arrows/Esc) preserved |
| NFR-010 | Deploy | Single binary + static assets; Caddy TLS like excel-agent-app |
| NFR-011 | AWP | Version negotiation middleware on all routes; health state reflects MCP child status |

---

## 7. System design

### 7.1 Repository layout (target)

```
spatial-os/
├── Cargo.toml
├── business.toml               # AWP site config (brand voice, capabilities, policies)
├── src/
│   ├── main.rs                 # entry; .merge(awp_routes).merge(api_routes)
│   ├── config.rs               # env, MCP paths, feature flags
│   ├── state.rs                # AppState: runner, sessions, ambient registry
│   ├── agents/
│   │   ├── mod.rs
│   │   ├── suzy.rs             # coordinator
│   │   ├── router.rs           # LlmConditionalAgent
│   │   ├── scenarios/          # morning.rs, deck.rs, …
│   │   └── ambient/            # research, scout, maker
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── mcp.rs              # spawn worksheet-mcp, docx-mcp, slides-mcp-server
│   │   └── adapters/           # thin wrappers only; prefer MCP toolsets
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── session.rs
│   │   ├── intent.rs           # POST → SSE
│   │   ├── action.rs           # fuse, commit, conduct
│   │   ├── agents.rs           # rails CRUD
│   │   ├── ambient.rs          # proactive stream
│   │   ├── artifacts.rs
│   │   └── signup.rs           # optional proxy
│   ├── events/
│   │   ├── mod.rs
│   │   └── sse.rs              # Event → SSE JSON mapping
│   └── voice/
│       ├── mod.rs
│       └── realtime.rs         # optional Gemini Live WS
├── field.html                  # preserved; gains field-client.js include
├── static/
│   └── field-client.js         # SSE bridge (extracted from inline script)
├── audio/
├── gen_audio.py
├── migrations/                 # Postgres (later milestones)
├── docs/
│   └── SPECIFICATION.md        # this file
└── deploy/
    └── Caddyfile
```

### 7.2 Agent topology

#### Coordinator (Suzy)

- **Role:** Summarize completed scenarios, arm suggestions, handle ambiguous intents.
- **Implementation:** `LlmAgent` with read-only session tools (`get_session_summary`, `list_artifacts`).
- **Output:** HTML summary + optional `suggest_next` payload.

#### Router

- **Role:** Map intent → scenario key.
- **Implementation:** `LlmConditionalAgent` with routes for each scenario + `default_route` (clarifying question).

#### Scenario workflows (per milestone expansion)

```
deck:
  ParallelAgent [excel_agent, docs_agent]
    → SequentialAgent [slides_agent (deps: excel, docs; toolset: slides-mcp-server)]

morning:
  ParallelAgent [calendar_agent, inbox_agent]
    → SequentialAgent [brief_agent (deps: both)]

lisbon:
  ParallelAgent [flights_agent, stay_agent]
    → SequentialAgent [planner_agent]

week / people / live / proactive:
  ParallelAgent [card1, card2, card3]  # third may waitsFor: 2
```

#### Ambient agents

| Agent | Trigger | Output |
|-------|---------|--------|
| research.agent | Cron `0 */30 * * * *` | Brief artifact + proactive tile |
| scout.agent | Webhook / price poll | Alert + proactive tile |
| maker.agent | Cron / user preference | Playlist, sketches, shortlists |

### 7.3 AWP integration (additive, not a second stack)

Implementation follows `examples/awp_agent` — estimated **~50 lines** in `main.rs` beyond route registration.

```rust
// main.rs (conceptual)
let awp_loader = BusinessContextLoader::from_file("business.toml")?;
let awp_state = AwpState::builder(awp_loader.context_ref()).build();

let app = Router::new()
    .merge(awp_routes(awp_state))      // /.well-known/awp.json, /awp/*
    .merge(api_routes(app_state))      // /api/sessions, /api/intent, …
    .fallback_service(ServeDir::new("."));
```

**`business.toml` capabilities** (grow with milestones; manifest is the live API catalog):

| Capability name | Endpoint | Trust | Milestone |
|-----------------|----------|-------|-----------|
| `submit_intent` | `POST /api/sessions/{sid}/intent` | known | M0 |
| `submit_action` | `POST /api/sessions/{sid}/action` | known | M2 |
| `fuse_cards` | `POST /api/sessions/{sid}/fuse` | known | M2 |
| `get_greeting` | `GET /api/greeting` | anonymous | M3 |
| `subscribe_proactive` | `POST /awp/events/subscribe` | partner | M7 |
| `list_artifacts` | `GET /artifacts/{path}` | known | M1 |

**A2A handler:** `POST /awp/a2a` parses `AwpTypedMessage`, maps message type → existing handler (e.g. `IntentSubmit` → `intent.rs`). Suzy `brand_voice` in `business.toml` aligns with prerecorded clip persona.

**Events:** When ambient agents complete (M7), emit AWP event → deliver to webhook subscribers. This is how external agents "watch" Agentrix OS work while the user lives.

**Dependencies:** `adk-awp`, `awp-types` (path deps in `Cargo.toml`, same as `adk-rust` workspace).

### 7.4 Session state schema

Keys use adk-core prefixes:

```json
{
  "app:scenario": "deck",
  "app:intent": "Build me a pitch deck",
  "app:cards": {
    "auto-excel": { "status": "done", "agent": "auto-excel", "artifact_id": "…" }
  },
  "app:agents:active": ["auto-excel", "auto-docs"],
  "app:agents:resting": ["inbox.agent"],
  "app:artifacts": { "deck-v1": { "path": "…", "mime": "…" } },
  "app:conduct:pending": null,
  "user:dnd": false,
  "user:preferences": { "voice": "Aoede", "tour_progress": 3 }
}
```

### 7.5 SSE event contract

All server → client events on `POST /api/sessions/{sid}/intent` (or GET SSE channel).

| Event `type` | Payload fields | UI handler |
|--------------|----------------|------------|
| `scenario` | `key` | set `currentKey` |
| `card_spawn` | `id`, `glyph`, `title`, `agent`, `delay`, `attention?`, `surface?` | `buildCard` + animate |
| `card_status` | `id`, `text` | update status line |
| `card_surface` | `id`, `surface`, `delta` | `buildSurface` partial update |
| `card_resolve` | `id`, `resolve`, `detail?` | `resolve()` |
| `suzy_summary` | `html`, `audio_clip?` | `showSuzy()` |
| `suggest` | `text`, `kind`: `action` \| `scenario` | `armSuggestion()` |
| `conduct` | `steps[]`: `{op, source?, target?}` | `conductAction()` server-driven |
| `agent_active` | `id`, `title`, `glyph` | `agentActive()` |
| `agent_done` | `id` | `agentDone()` |
| `error` | `message`, `recoverable` | toast + log |
| `done` | — | close stream |

### 7.6 REST API (complete)

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/sessions` | Create session → `{ session_id, user_id }` |
| `POST` | `/api/sessions/{sid}/intent` | Submit intent → SSE stream |
| `POST` | `/api/sessions/{sid}/action` | Action turn (`combine`, `handle it`, …) → SSE |
| `POST` | `/api/sessions/{sid}/fuse` | `{ source_card_id, target_card_id }` |
| `POST` | `/api/sessions/{sid}/commit` | `{ card_id, action_label }` |
| `POST` | `/api/agents/{id}/snooze` | Fling → resting |
| `POST` | `/api/agents/{id}/wake` | Wake from resting |
| `GET` | `/api/sessions/{sid}/agents` | Hydrate rails |
| `GET` | `/api/sessions/{sid}/cards` | Hydrate field on reload |
| `GET` | `/api/ambient` | SSE proactive updates |
| `GET` | `/api/people` | People rail data |
| `GET` | `/api/live` | Live feed slides |
| `GET` | `/artifacts/{path}` | Serve artifacts (auth scoped) |
| `GET` | `/health` | Health check |

### 7.7 UI actualization map

| Current `field.html` function | Production behavior |
|------------------------------|---------------------|
| `pickScenario(text)` | Server `scenario` event (client fallback only in demo mode) |
| `launch(text)` | `POST intent` → consume SSE `card_spawn` |
| `runCard(card, s, spec)` | SSE `card_status` + `card_resolve` |
| `buildSurface(body, type)` | SSE `card_surface` with preview payloads |
| `showSuzy()` | SSE `suzy_summary` |
| `conductAction()` | SSE `conduct` steps + `POST action` |
| `fuse(target, source)` | `POST fuse` + SSE updates |
| `commitAction(...)` | `POST commit` |
| `snoozeAgent` / `wakeAgent` | `POST snooze` / `wake` |
| `renderPeople()` | `GET /api/people` |
| Live feed `sources[]` | `GET /api/live` |
| `proactiveAgents[]` | `GET /api/ambient` + SSE |
| `playGreeting()` | `GET /api/greeting` → personalized text + audio URL |

**Demo mode preservation (NFR-008):**

```javascript
const DEMO = new URLSearchParams(location.search).has('demo') || !window.__AGENTRIX_API__;
// if DEMO: use existing scenarios{} path unchanged
```

---

## 8. Technology stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Agent framework | adk-rust (path dep) | Existing Zavora stack |
| Agentic Web Protocol | adk-awp + awp-types + `business.toml` | Reference AWP demo; `awp_routes()` merge |
| HTTP server | Axum + adk-server `ServerBuilder` | Consistent with sibling apps |
| Streaming | SSE | docx-agent-app proven pattern |
| Live voice (final) | adk-realtime + WebSocket | mia proven pattern |
| Spreadsheets | worksheet-mcp + agentrix-xlsx | excel-agent-app |
| Documents | docx-mcp | docx-agent-app |
| Presentations | slides-mcp-server + agentrix-slide | `mcp-servers/mcp_slides` (~72 tools, .pptx/.pdf) |
| Calendar | mcp-calendar | Google Calendar + Microsoft Graph |
| Email | mcp-email | Gmail, Graph, IMAP, SMTP (~24 tools) |
| News & briefs | mcp-news | GDELT (free), GNews, HN, sports, partial markets |
| Personal finance | mcp-banking | Plaid, Mono, Open Banking UK/EU |
| Markets | mcp-market-data | Quotes, history, analytics (when portfolio depth needed) |
| Team chat | mcp-slack | Channels, DMs, search, scheduled messages |
| CRM | mcp-crm | Salesforce, HubSpot, Zoho, Pipedrive |
| Dev focus | mcp-github | Commits, PRs, issues |
| Maps & routing | mcp-maps | OSM geocoding, OSRM routing (free) |
| Weather | mcp-weather | Open-Meteo forecasts (free) |
| Property scout | mcp-real-estate | Listings, valuations, price indicators |
| Research retrieval | mcp-search | Full-text + semantic search (needs index backend) |
| Desktop fallback | computer-use-mcp | Apps without MCP (e.g. airline booking) |
| Sessions (prod) | adk-session + PostgreSQL | excel-agent-app |
| Auth (prod) | JWT cookies | excel-agent-app |
| Frontend | field.html + field-client.js | Preserve design |
| Voice clips | gen_audio.py + Gemini TTS | Cost control |
| Deploy | Caddy + systemd | excel-agent-app |

### 8.1 MCP server paths (local dev)

MCP children are spawned per **boot phase** (see §11.1). Paths are configurable; defaults assume sibling `mcp-servers/` checkout.

| Variable | Binary | Repo path | Boot phase |
|----------|--------|-----------|------------|
| `MCP_WORKSHEET_PATH` | `excel-mcp-server` | `mcp-servers/worksheet-mcp/mcp-server` | A |
| `MCP_DOCX_PATH` | `docx-mcp-server` | `mcp-servers/docx-mcp` | A |
| `MCP_SLIDES_PATH` | `slides-mcp-server` | `mcp-servers/mcp_slides` | A |
| `MCP_CALENDAR_PATH` | `mcp-calendar` | `mcp-servers/mcp-calendar` | B |
| `MCP_EMAIL_PATH` | `mcp-email` | `mcp-servers/mcp-email` | B |
| `MCP_NEWS_PATH` | `mcp-news` | `mcp-servers/mcp-news` | B |
| `MCP_WEATHER_PATH` | `mcp-weather` | `mcp-servers/mcp-weather` | B |
| `MCP_BANKING_PATH` | `mcp-banking` | `mcp-servers/mcp-banking` | E |
| `MCP_GITHUB_PATH` | `mcp-github` | `mcp-servers/mcp-github` | E |
| `MCP_SLACK_PATH` | `mcp-slack` | `mcp-servers/mcp-slack` | D |
| `MCP_CRM_PATH` | `mcp-crm` | `mcp-servers/mcp-crm` | D |
| `MCP_MARKET_DATA_PATH` | `mcp-market-data` | `mcp-servers/mcp-market-data` | C |
| `MCP_MAPS_PATH` | `mcp-maps` | `mcp-servers/mcp-maps` | F |
| `MCP_REAL_ESTATE_PATH` | `mcp-real-estate` | `mcp-servers/mcp-real-estate` | F |
| `MCP_SEARCH_PATH` | `mcp-search` | `mcp-servers/mcp-search` | G |
| `MCP_COMPUTER_USE_PATH` | `@zavora-ai/computer-use-mcp` | `mcp-servers/computer-use-mcp` | F (fallback) |

**Tool scoping:** Each sub-agent receives an allowlisted subset of its MCP server's tools (BK-005). Do not mount all tools on Suzy or the router.

**Overlap:** `mcp-news` includes `yfinance_chart` and `crypto_prices` — sufficient for basic Live/Markets cards; add `mcp-market-data` only when portfolio analytics exceed quote-level data.

---

## 9. Agile milestones

Each milestone:

1. Lives on a branch `milestone/Mx-short-name`.
2. Ends with **squash or merge commit** to `main` after user validation.
3. Includes **automated checks** (`cargo test`, `cargo clippy`, manual demo script).
4. Does **not** remove demo mode until M8.

### M0 — Live shell (backend skeleton)

**Goal:** Server serves UI; session created; intent returns SSE replay of one scenario (mock events from server, not browser timers).

| Task | Detail |
|------|--------|
| M0-T1 | `src/main.rs` — Axum, serve `field.html`, `audio/`, `static/` |
| M0-T2 | `POST /api/sessions` — in-memory session |
| M0-T3 | `POST /api/sessions/{sid}/intent` — SSE emitter with `deck` scenario events |
| M0-T4 | Extract `field-client.js` — SSE consumer calls existing `buildCard`/`resolve` |
| M0-T5 | Feature flag `?demo=1` keeps legacy path |
| M0-T6 | `cargo run` README snippet in spec appendix |
| M0-T7 | **AWP shell:** `business.toml` (core capabilities) + `awp_routes()` merge; `GET /.well-known/awp.json` returns discovery doc |

**Validation script (user):**

1. `cargo run` → open `http://localhost:8080`
2. Submit "Build me a pitch deck"
3. Confirm cards bloom from network tab SSE (not `setInterval`)
4. Confirm `?demo=1` still runs offline simulation
5. Confirm greeting + signup still work
6. `curl localhost:8080/.well-known/awp.json` returns valid discovery document
7. `curl localhost:8080/awp/manifest` lists `submit_intent` capability

**Suggested commits:** `feat(server): axum static shell`, `feat(api): session + intent SSE`, `feat(ui): field-client SSE bridge`, `feat(awp): business.toml + awp_routes merge`

---

### M1 — Real deck workflow

**Goal:** All three Auto-* cards produce real artifacts via MCP; Auto-Slides waits for Excel + Docs.

| Task | Detail |
|------|--------|
| M1-T1 | MCP bootstrap (`mcp.rs`) — spawn **three** child processes at boot: `worksheet-mcp`, `docx-mcp`, `slides-mcp-server` (binary: `slides-mcp-server` from `mcp-servers/mcp_slides`) |
| M1-T2 | `excel_agent` → worksheet-mcp toolset; `docs_agent` → docx-mcp toolset; **`slides_agent` → slides-mcp-server toolset** |
| M1-T3 | `DeckWorkflow` — `ParallelAgent[excel, docs]` → `SequentialAgent[slides]` (slides blocked until `waitsFor: 2` satisfied) |
| M1-T4 | Map adk-runner events → SSE `card_status` / `card_resolve` |
| M1-T5 | Artifact storage + `GET /artifacts/...` (.xlsx, .docx, **.pptx**) |
| M1-T6 | Auto-surface preview payloads in `card_surface` events — slides filmstrip fed by `describe_presentation` slide count + incremental `add_slide` tool events |
| M1-T7 | `slides_agent` system prompt: ingest excel/docx artifact paths from session state; use `create_presentation` (or `business:pitch` template) → populate slides → `save_presentation` |

**slides-mcp-server key tools for Auto-Slides:**

| Tool | Role in deck scenario |
|------|----------------------|
| `create_presentation` | Blank or `business:pitch` template deck |
| `add_slide` / `set_title` / `add_bullets` | Build slide content from docx narrative |
| `add_table` / `set_table_cell` | Embed Q3 numbers from excel context |
| `add_chart` | Revenue chart from excel data (when agent extracts figures) |
| `describe_presentation` | Slide count for filmstrip UI + resolve copy ("10 slides") |
| `render_slide` | Optional PNG thumbnails for `surface: slides` filmstrip |
| `save_presentation` | Write `.pptx` to `artifacts/{session}/` |
| `to_markdown` | Coordinator/Suzy summary of deck structure |

**Validation:**

1. "Build me a pitch deck" → downloadable `.xlsx`, `.docx`, and **`.pptx`** in artifact store
2. Auto-Slides card stays `composing…` / `waiting…` until excel + docs complete
3. Slides filmstrip increments as `add_slide` tool calls stream (not CSS-only animation)
4. Open/Save on Auto-Slides returns real `.pptx` URL
5. Refresh page — cards lost (persistence is M5)

**Commits:** `feat(mcp): spawn worksheet, docx, and slides servers`, `feat(agents): deck workflow with slides_agent`, `feat(ui): auto-surface preview from SSE`

---

### M2 — Action turn: combine

**Goal:** "combine" produces merged deck artifact; Conducting matches server steps.

| Task | Detail |
|------|--------|
| M2-T1 | `POST /api/sessions/{sid}/action` — detect action verbs |
| M2-T2 | `combine_agent` — reads excel + docs artifacts, **updates Auto-Slides `.pptx` via slides-mcp-server** (merge numbers into tables/charts, narrative into slides, dedupe/reflow) |
| M2-T3 | SSE `conduct` event sequence for hand animation |
| M2-T4 | `finishDeck()` wired to real pinned artifact + `deck_done` audio |
| M2-T5 | `POST /api/sessions/{sid}/fuse` for generic fuse |

**Validation:**

1. Run deck scenario → say "combine"
2. Hand conducts fuses; pinned Auto-Slides card shows real merged output
3. Final **`.pptx`** downloadable; slide count matches `describe_presentation` + UI copy
4. Excel figures appear in slide tables/charts; docx narrative appears in slide titles/bullets

---

### M3 — Morning scenario (calendar + inbox + brief)

**Goal:** First multi-integration daily workflow via MCP.

| Task | Detail |
|------|--------|
| M3-T1 | Boot **Phase B** MCP: `mcp-calendar`, `mcp-email`, `mcp-news`, `mcp-weather` |
| M3-T2 | `calendar_agent` — `get_today`, `list_events`, `find_free_time` |
| M3-T3 | `inbox_agent` — `list_inbox`, `search_emails`, `create_draft`; commit maps to `send_draft` / `reply_to_email` |
| M3-T4 | `brief_agent` — `gnews_top_headlines`, `search_news`; `mcp-weather` `get_forecast` for commute line |
| M3-T5 | `waitsFor` brief card (after calendar + inbox complete) |
| M3-T6 | `GET /api/greeting` — personalized from `calendar_agent` output + optional clip regen |

**Validation:**

1. OAuth/connect test Google account via mcp-calendar + mcp-email
2. "Start my day" → real meeting count + real flagged emails (not static copy)
3. Brief includes live headline + weather line for user's location
4. Greeting clip/text reflects actual calendar

---

### M4 — VerbUI persistence (snooze / wake / commit)

**Goal:** Rails and field survive refresh; commit executes server actions.

| Task | Detail |
|------|--------|
| M4-T1 | Session state for cards + rails |
| M4-T2 | `GET /api/sessions/{sid}/cards` hydrate on load |
| M4-T3 | snooze/wake endpoints |
| M4-T4 | commit endpoint — map labels → tool calls (draft email, hold flight, …) |

**Validation:**

1. Fling card → refresh → still in Resting
2. Wake → returns to Active
3. Primary commit on inbox card sends draft (or saves artifact)

---

### M5 — Suzy coordinator + router

**Goal:** Dynamic summaries; LLM scenario routing; guided tour from server.

| Task | Detail |
|------|--------|
| M5-T1 | `suzy.rs` coordinator agent |
| M5-T2 | `router.rs` LlmConditionalAgent |
| M5-T3 | Replace static `suzySummaries{}` |
| M5-T4 | `suggest` events for tour |
| M5-T5 | Clip matching — if summary matches known clip, play wav; else `gen_audio` queue or live TTS |

**Validation:**

1. Paraphrased intents route correctly ("pitch presentation" → deck)
2. Suzy summary text differs when underlying data differs
3. Tour suggestions advance logically

---

### M6 — Remaining scenarios

**Goal:** lisbon, week, people, live — all real via MCP (proactive ambient work completes in M7).

| Task | Detail |
|------|--------|
| M6-T1 | **live** — boot Phase C: `mcp-news` (+ optional `mcp-market-data`); Headlines/Markets/Now cards |
| M6-T2 | **people** — boot Phase D: `mcp-slack`, `mcp-crm`, `mcp-calendar`; Team/Priya/Connections cards |
| M6-T3 | **week** — boot Phase E: `mcp-banking`, `mcp-github`; Money + Focus cards; Health via manual import until wearable MCP exists |
| M6-T4 | **lisbon** — boot Phase F: `mcp-maps`, `mcp-weather`, `mcp-real-estate`; Itinerary card real; Flights/Stay via travel gap strategy (Appendix C §C.4) |
| M6-T5 | Shell rails: People (`mcp-slack` presence), Live (`mcp-news` carousel) |

**Validation:** Run full `tourOrder` script; each scenario produces non-static evidence (MCP tool log, artifact, or API row). Lisbon Flights/Stay may use computer-use fallback until `mcp-travel` exists — must be labeled in UI, not silent simulation.

---

### M7 — Ambient proactive layer

**Goal:** Background agents populate proactive rail without user intent.

| Task | Detail |
|------|--------|
| M7-T1 | Boot **Phase G**: `mcp-news`, `mcp-real-estate`, `mcp-search` (+ `media-mcp` for Maker if ready) |
| M7-T2 | AmbientAgent × 3 — research (`arxiv_search`, `search_news`), scout (`us_search_properties`, `yfinance_chart`), maker (creative tools) |
| M7-T3 | `GET /api/ambient` SSE |
| M7-T4 | DND respect (`user:dnd` suppresses card bloom) |
| M7-T5 | Background cards reflect ambient status |
| M7-T6 | Optional: `mcp-notifications` push when scout finds price drop |

**Validation:**

1. Leave app open 30+ min (or trigger cron manually)
2. Proactive rail updates with completed work
3. "Show me what you found" blooms proactive scenario from real stored results

---

### M8 — Shell rails polish

**Goal:** Rails stay fresh without full scenario bloom; background cards server-driven.

| Task | Detail |
|------|--------|
| M8-T1 | People rail — `mcp-slack` `list_users`, `list_dms`, presence polling |
| M8-T2 | Live feed — `mcp-news` `gnews_top_headlines`, `hn_stories`, `sports_news` (swipe carousel) |
| M8-T3 | Background card intents from server config (not hardcoded `bgcards[]`) |
| M8-T4 | Optional: social feed gap — X/IG equivalents deferred (Appendix C §C.4) |

**Validation:** Slack activity change → People rail updates within polling/SSE interval.

---

### M9 — Auth + Postgres persistence

**Goal:** Multi-device sessions; production data model.

| Task | Detail |
|------|--------|
| M9-T1 | migrations from excel-agent-app pattern |
| M9-T2 | `pg_session.rs` |
| M9-T3 | Auth (Google OAuth minimum) |
| M9-T4 | Artifact paths per `user_id` |

**Validation:** Login on two browsers — same session id shows same cards/rails.

---

### M10 — Live voice Suzy

**Goal:** Replace prerecorded-only with Gemini Live for greeting + summary (clips remain fallback).

| Task | Detail |
|------|--------|
| M10-T1 | WebSocket `/ws/voice` — mia pattern |
| M10-T2 | Mic stream → adk-realtime |
| M10-T3 | Tool calls during voice |
| M10-T4 | Fallback to wav clips on WS failure |
| M10-T5 | Camera channel: client sends `{type:"frame", mime, data}` (JPEG, ≈1 fps, gated at 2.5 fps / 256 KB) up `/ws/voice`; Suzy reports deliberate gestures with the `ui_gesture` tool, relayed as `tool_call`; `gestures.js` maps swipe → world pager, open palm → `POST /api/pause`, wave → briefing intent. Frames are never stored or logged; flag `AGENTRIX_CAMERA`; `/api/voice/status` exposes `camera` |

**Validation:** Voice conversation with Suzy updates calendar card in real time.

---

### M11 — Deployed prototype

**Goal:** Public URL; HTTPS; demo capture works against hosted site.

| Task | Detail |
|------|--------|
| M11-T1 | Dockerfile + Caddy |
| M11-T2 | CI: `cargo test` + `cargo clippy` |
| M11-T3 | `capture.js` targets `https://…` |
| M11-T4 | Environment secrets documented |
| M11-T5 | AWP conformance run against production URL; link `/.well-known/awp.json` from early-access page footer |

**Validation:** Early-access page on production URL passes full tour on desktop + mobile Safari. External agent via `POST /awp/a2a` completes deck scenario without browser.

---

## 10. Master task backlog

Tasks not tied to a single milestone (ongoing):

| ID | Task | Owner | Depends |
|----|------|-------|---------|
| BK-001 | Extract `field-client.js` from `field.html` without visual regression | FE | M0 |
| BK-002 | Playwright screenshot diff test for card bloom | FE | M0 |
| BK-003 | SSE event schema crate or TypeScript types for contract tests | BE | M0 |
| BK-004 | Integration test: deck intent → `.xlsx`, `.docx`, and `.pptx` artifact files exist | BE | M1 |
| BK-005 | Tool allowlist per sub-agent (security) | BE | M1 |
| BK-006 | Rate limiting on `/api/intent` | BE | M9 |
| BK-007 | Regenerate audio clips script in CI when copy changes | Ops | M5 |
| BK-008 | LinkedIn conversion still fires after SPA API mode | Mkt | M0 |
| BK-009 | Design + build `mcp-travel` (flights + lodging) or document permanent computer-use path | BE | M6 |
| BK-010 | Health data ingest path (HealthKit export / CSV) until wearable MCP exists | BE | M6 |
| BK-011 | `mcp-registry` allowlist config per sub-agent | BE | M9 |
| BK-012 | A2A message type → intent/action handler routing table | BE | M0 |
| BK-013 | Expand `business.toml` capabilities as milestones ship (manifest stays in sync) | BE | ongoing |

---

## 11. MCP integration plan

Agentrix OS v1.0 wires **~12 MCP servers** (not the full `mcp-servers` monorepo). Enterprise verticals (ERP, SCADA, EHR, fraud, LIMS, etc.) are out of scope — see Appendix C §C.3.

### 11.1 Phased boot order

MCP children are warmed at server boot per milestone phase. Later phases add processes; earlier phases stay running.

| Phase | Milestone | MCP servers spawned | Scenarios unlocked |
|-------|-----------|---------------------|-------------------|
| **A** | M1–M2 | `worksheet-mcp`, `docx-mcp`, `slides-mcp-server` | deck |
| **B** | M3 | `mcp-calendar`, `mcp-email`, `mcp-news`, `mcp-weather` | morning |
| **C** | M6 | `mcp-market-data` (optional if `mcp-news` quotes suffice) | live (markets depth) |
| **D** | M6 | `mcp-slack`, `mcp-crm` | people |
| **E** | M6 | `mcp-banking`, `mcp-github` | week |
| **F** | M6 | `mcp-maps`, `mcp-real-estate`, `computer-use-mcp` (fallback) | lisbon (partial) |
| **G** | M7 | `mcp-search`, `mcp-notifications` (optional), `media-mcp` (optional) | proactive |
| — | M10 | Gemini Live (adk-realtime, not MCP) | voice |

### 11.2 Integration dependency matrix

| MCP server | Cards / features | Milestone | Priority | Notes |
|------------|------------------|-----------|----------|-------|
| `worksheet-mcp` | Auto-Excel | M1 | P0 | Artifact .xlsx |
| `docx-mcp` | Auto-Docs | M1 | P0 | Artifact .docx |
| `slides-mcp-server` | Auto-Slides, combine | M1–M2 | P0 | Artifact .pptx |
| `mcp-calendar` | Today, Priya 1:1, greeting | M3, M6 | P0 | `get_today`, `find_free_time` |
| `mcp-email` | Needs you, draft/commit | M3 | P0 | `create_draft`, `reply_to_email` |
| `mcp-news` | Brief, Headlines, Now, Research | M3, M6, M7 | P1 | GDELT/GNews free; includes `yfinance_chart` |
| `mcp-weather` | Brief, Itinerary | M3, M6 | P1 | Open-Meteo, no API key |
| `mcp-slack` | Team, People rail | M6, M8 | P1 | Channel history, DMs, send |
| `mcp-crm` | Connections, reconnect | M6 | P1 | Contacts, activities, notes |
| `mcp-banking` | Money | M6 | P1 | Plaid/Mono; OAuth required |
| `mcp-github` | Focus | M6 | P1 | Commits, PRs |
| `mcp-market-data` | Markets (portfolio) | M6 | P2 | Skip if `mcp-news` quotes enough |
| `mcp-maps` | Itinerary | M6 | P2 | Routing, POI, geocoding |
| `mcp-real-estate` | Scout, Stay (partial) | M6, M7 | P2 | Listings ≠ hotel nightly booking |
| `mcp-search` | Research (deep) | M7 | P2 | Needs search index backend |
| `computer-use-mcp` | Flights book (fallback) | M6 | P2 | Until `mcp-travel` exists |
| `mcp-notifications` | Proactive alerts | M7 | P3 | Push on scout hit |
| `mcp-session-memory` | Suzy cross-session | M9 | P3 | User preferences |
| `mcp-registry` | Tool allowlists at scale | M9+ | P3 | When >10 MCP children |
| Gemini Live | Voice Suzy | M10 | — | adk-realtime, not MCP |

### 11.3 Catalog gaps (not yet in mcp-servers)

| Demo feature | Status | v1.0 strategy |
|--------------|--------|---------------|
| Flight search / hold seat | No `mcp-travel` | `computer-use-mcp` or stub with explicit UI label until built |
| Hotel / nightly stay | `mcp-real-estate` ≠ lodging | Same as flights; track as BK-009 |
| Personal health (sleep, steps) | No wearable MCP | Manual HealthKit export ingest; track as BK-010 |
| Social graphs (X, Instagram) | Not in `mcp-news` | Defer People rail social lines; news/HN only in v1 |

---

## 12. User validation process

### 12.1 Per-milestone gate

1. Developer completes milestone branch.
2. Developer runs automated checks locally.
3. Developer records **screen capture** of validation script.
4. User (product owner) executes script on branch build.
5. User marks checklist **pass/fail** in GitHub issue or PR description.
6. **Fail** → fix on branch; **pass** → merge to `main`, tag `v0.Mx.0`.

### 12.2 Regression suite (accumulates)

After M2, every release re-runs:

- [ ] Greeting → start day → scenario blooms
- [ ] Deck → combine → pinned artifact
- [ ] Demo mode `?demo=1` offline
- [ ] Early-access signup POST
- [ ] Mobile stacked layout (≤900px) — no coverflow break
- [ ] Voice input submits intent (Chrome)

---

## 13. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| OAuth complexity delays M3/M6 | High | mcp-calendar + mcp-email + mcp-banking share OAuth patterns; fixture accounts for CI |
| 12+ MCP children memory/startup | Medium | Phased boot; lazy spawn per scenario; mcp-registry health checks |
| Travel + health catalog gaps | Medium | computer-use fallback + labeled stubs (§11.3); BK-009, BK-010 |
| SSE + long MCP runs disconnect | Medium | Heartbeat events; resume from `app:cards` state |
| Monolithic HTML merge conflicts | Medium | Extract `field-client.js` early (M0) |
| Live voice cost | Low | Clips default; live opt-in |
| VerbUI on mobile disabled | Low | Server-driven conduct works without drag on stacked tier |

---

## 14. Definition of done (v1.0 GA)

Agentrix OS v1.0 is complete when:

- [ ] All FR-001–FR-087 satisfied
- [ ] All NFR-001–NFR-011 satisfied
- [ ] Milestones M0–M11 merged and tagged
- [ ] Regression suite passes on production URL
- [ ] No production code path uses static `scenarios{}` or fake `setInterval` streams
- [ ] Demo mode explicitly labeled "simulated" in UI for marketing captures

---

## Appendix A — Suggested first sprint (M0 only)

**Sprint goal:** Prove the UI can be driven by the server without changing how it looks.

| Day | Work |
|-----|------|
| 1 | `main.rs`, static serving, health route |
| 2 | Session + intent SSE (hardcoded deck events) |
| 3 | `field-client.js` extraction + SSE wiring |
| 4 | Demo mode flag + validation recording |
| 5 | User validation + merge |

**Branch:** `milestone/M0-live-shell`  
**Tag after merge:** `v0.0.1-m0`

---

## Appendix C — MCP integration assessment

Survey of `mcp-servers/` against Agentrix OS scenarios. Full analysis used to derive §11.

### C.1 Priority tiers

| Tier | Servers | Rationale |
|------|---------|-----------|
| **P0** | worksheet-mcp, docx-mcp, slides-mcp-server, mcp-calendar, mcp-email | Deck + morning are the demo spine; cards cannot be real without these |
| **P1** | mcp-news, mcp-weather, mcp-slack, mcp-crm, mcp-banking, mcp-github | Cover live, people, week scenarios with owned MCP |
| **P2** | mcp-maps, mcp-real-estate, mcp-market-data, mcp-search, computer-use-mcp | Lisbon itinerary, scout, portfolio depth, research |
| **P3** | mcp-notifications, mcp-session-memory, mcp-registry, media-mcp | Proactive alerts, memory, ops at scale |
| **Skip v1** | ERP, SCADA, EHR, fraud, warranty, LIMS, student-records, … | Enterprise vertical agents — wrong product surface |

### C.2 Scenario → MCP mapping

**Morning**

| Card | MCP | Key tools |
|------|-----|-----------|
| Today | mcp-calendar | `get_today`, `list_events`, `find_free_time` |
| Needs you | mcp-email | `list_inbox`, `search_emails`, `create_draft`, `reply_to_email` |
| Brief | mcp-news + mcp-weather | `gnews_top_headlines`, `search_news`; `get_forecast` |

**Deck** — worksheet-mcp, docx-mcp, slides-mcp-server (see §5.6, M1).

**Week**

| Card | MCP | Notes |
|------|-----|-------|
| Money | mcp-banking | `list_transactions`, `search_transactions`, `categorize_transaction` |
| Health | — | **Gap:** mcp-medical is clinical (PubMed/WHO), not wearables |
| Focus | mcp-github | `list_commits`, `list_pull_requests` |

**People**

| Card | MCP | Key tools |
|------|-----|-----------|
| Team | mcp-slack | `get_channel_history`, `search_messages`, `send_message` |
| Priya | mcp-calendar + mcp-crm | `list_events`, `list_notes`, `create_note` |
| Connections | mcp-crm | `search_contacts`, `list_activities` |

**Live**

| Card | MCP | Notes |
|------|-----|-------|
| Headlines | mcp-news | `gnews_top_headlines`, `get_trending_topics` |
| Markets | mcp-news and/or mcp-market-data | news has `yfinance_chart`; market-data for portfolio analytics |
| Now | mcp-news | `sports_news`, `hn_stories`, `nasa_events` |

**Lisbon**

| Card | MCP | Notes |
|------|-----|-------|
| Flights | **Gap** | mcp-logistics is shipping, not airlines |
| Stay | mcp-real-estate (partial) | Property listings ≠ nightly hotels |
| Itinerary | mcp-maps + mcp-weather | `optimize_route`, POI search, forecasts |

**Proactive**

| Agent | MCP | Key tools |
|-------|-----|-----------|
| Research | mcp-news | `arxiv_search`, `search_news` |
| Scout | mcp-real-estate + mcp-news | price/listing search, `yfinance_chart` |
| Maker | media-mcp | Playlists, generated content (optional) |

### C.3 Servers that do not add consumer OS value

The monorepo contains 100+ MCP servers for regulated industries and enterprise workflows. None map to Agentrix OS v1 scenarios unless the product pivots: mcp-erp, mcp-scada, mcp-ehr, mcp-fraud, mcp-warranty, mcp-lims, mcp-student-records, mcp-pharmacy, mcp-credit-bureau, etc.

### C.4 Catalog gaps and mitigations

| Gap | Mitigation |
|-----|------------|
| Flights / hold seat | Build `mcp-travel` (BK-009) or `computer-use-mcp` with explicit UI label |
| Hotel nightly booking | Same as flights; mcp-real-estate covers property scout only |
| Wearable health | BK-010: HealthKit/CSV ingest adapter |
| Social presence (X, IG) | Defer; mcp-news covers headlines not social graphs |
| Family People rail | Partial via mcp-slack DMs; contacts MCP TBD |

### C.5 Key insight

**`mcp-news` is high leverage** — one server covers morning brief, live headlines, partial markets, sports/live-ish content, and research (`arxiv_search`), mostly on free tiers (GDELT, GNews, HN). Prefer news before adding redundant market APIs.

---

## Appendix D — AWP demonstration narrative

Use this story when presenting Agentrix OS as an AWP reference implementation.

### D.1 The problem AWP solves here

Without AWP, every external agent must reverse-engineer `field.html` or undocumented `/api/*` routes. With AWP, an agent:

1. Fetches `/.well-known/awp.json`
2. Reads `/awp/manifest` for capability names, endpoints, trust levels
3. Subscribes to proactive events (optional)
4. Sends `POST /awp/a2a` with a typed intent message
5. Receives the same deck/morning/people outcomes the human sees as cards

### D.2 Demo script (two consumers, one server)

```
Human path:  Browser → field.html → POST /api/intent → SSE card bloom
Agent path:  curl /.well-known/awp.json → POST /awp/a2a → same orchestration
```

Both paths should produce artifacts in `artifacts/{session}/` and update session state identically.

### D.3 What not to do (keeps complexity low)

- Do **not** build a separate AWP-only server or duplicate agents.
- Do **not** expose MCP tools directly over AWP — MCP stays internal; AWP exposes **orchestration capabilities** (`submit_intent`, `submit_action`).
- Do **not** block M1–M3 on AWP — M0-T7 ships the shell; capabilities accrue in `business.toml` as milestones land.

### D.4 Suggested `business.toml` header

```toml
site_name = "Agentrix OS"
site_description = "The agentic OS that works while you live — spatial agent orchestration for humans and AI agents."
domain = "zavora.ai"
contact = "hello@zavora.ai"

[brand_voice]
tone = "warm, confident, quietly witty — like a sharp friend who's glad to help"
greeting = "Good morning. Here's your day."
escalation = "This needs your attention — I've surfaced it on the field."

[[policies]]
name = "privacy"
description = "Session-scoped agent state. OAuth tokens never exposed via AWP anonymous tier."
policy_type = "privacy"
```

---

## Appendix B — Document history

| Version | Date | Change |
|---------|------|--------|
| 1.0 | 2026-06-22 | Initial full specification |
| 1.1 | 2026-06-22 | MCP integration assessment (Appendix C), phased boot order (§11), milestone alignment |
| 1.2 | 2026-06-22 | AWP reference implementation role (§2.1, §7.3, FR-083–087, Appendix D) |
| 1.2.1 | 2026-06-22 | Linked `IMPLEMENTATION_PLAN.md` task checklist |