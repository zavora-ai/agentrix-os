# Agentrix OS 1.0 — Base agents and companion apps

**Status:** Proposed (2026-10-17) · Part of [`AGENTIC_OS.md`](AGENTIC_OS.md) · Manifest format §3.2
of that document · Effect classes from ADR-003.

The OS ships with one coordinator, twelve world agents and two system agents. Each is a compiled
binary in `agents/<id>/` with a companion app in `crates/agentrix-apps/<app>/` and a shell module in
`shell/src/apps/<app>/`. More agents install from the Store.

**Design rules for every agent**

1. **Real or nothing.** Every capability is backed by a tool that works on local data, a connected
   integration, or it is absent and the agent says so. No sample data outside the labeled sample space.
2. **Deterministic where possible.** Arithmetic, time, availability, parsing, ranking and diffing
   are Rust tools, not model reasoning. The model plans, writes and judges.
3. **Least privilege.** The manifest lists syscall namespaces and memory scopes; the kernel
   enforces them. Unknown tools are `send_external` (ADR-003).
4. **One surface per answer.** Every task ends in a surface from the catalogue (AGENTIC_OS §10.4).
5. **Evals ship with the agent.** At least the 1.0 journeys it takes part in, as adk-eval trajectories
   on the record/replay gateway.

## 1. Inventory

| Agent | Worlds | Companion app | Works with no integration? | Integrations (optional) | Default mode | Model · effort |
|---|---|---|---|---|---|---|
| **Mother** | all | Today, Memory | yes | — | — (no world tools) | Fable 5.1 · medium / low |
| **Calendar** | work, home | Calendar | yes (local + `.ics`) | Google, Microsoft, CalDAV | suggest | Opus 5.5 · medium |
| **Mail** | work, home | Mail | read-only of imported `.mbox`/`.eml` | IMAP/SMTP, Gmail, Graph | suggest | Opus 5.5 · medium |
| **Tasks** | work, home | Tasks | yes | Todoist, Google Tasks (later) | suggest | Opus 5.5 · low |
| **Notes** | work, home | Notes | yes | Notion, Google Docs (later) | suggest | Opus 5.5 · medium |
| **Files & Docs** | work, home | Files | yes (chosen folders; docx/xlsx/pptx/pdf engines) | Drive, OneDrive (later) | suggest | Opus 5.5 · high |
| **People** | work, home | People | yes (built from mail, calendar, notes) | Google Contacts, CardDAV, Slack users | suggest | Opus 5.5 · low |
| **Research** | shared | Reader | needs only the Anthropic key (web search) | mcp-news, mcp-browser | suggest | Opus 5.5 · high |
| **Team** | work | — (surfaces in Mail-like views) | no | Slack | suggest | Opus 5.5 · medium |
| **Finance** | home | Finance | yes (CSV import, budgets) | Plaid / Mono read-only | observe | Opus 5.5 · medium |
| **Health & Wellbeing** | home | Health | yes (exports, manual logs) | Apple Health export | observe | Opus 5.5 · medium |
| **Travel & Places** | home, work | — (trips live in Calendar, places in People) | yes (maps, weather keyless) | — | suggest | Opus 5.5 · medium |
| **Insights** | shared (reads both via Mother's scoped API) | Today (observations) | yes (ledger) | — | observe | Opus 5.5 · low |
| **Automations** (system) | shared | Trust Center → Recipes | yes | — | suggest | Opus 5.5 · low |
| **Studio** (system) | shared | Studio | yes | mcp-imagegen (local) for wallpapers | suggest | Opus 5.5 · medium |
| *Store:* Developer | work | — | no | GitHub (mcp-github), ACP to coding agents | suggest | Opus 5.5 · high |
| *Store:* Media | home | Media | — | media-mcp, mcp-imagegen | suggest | — |

## 2. Syscall catalogue (companion-app tools)

These are the kernel tools agents call. Integration tools (mcp-email, mcp-calendar …) sit behind
them as drivers, so agents use one vocabulary whether or not an integration is connected.

| Namespace | Tool | Effect |
|---|---|---|
| `calendar` | `list_events`, `get_event`, `free_busy`, `find_free_time` (honours profile rules), `conflicts` | read |
| | `create_event`, `update_event` (no attendees) | write_local |
| | `create_event` / `update_event` with attendees, `rsvp` | schedule_with_others |
| | `delete_event` | delete |
| | `import_ics`, `export_ics` | write_local / read |
| `mail` | `search`, `get_thread`, `list_needs_reply`, `attachments` | read |
| | `create_draft`, `update_draft`, `label`, `archive`, `mark` | write_local |
| | `send_draft`, `reply`, `forward` | send_external |
| | `delete` | delete |
| `tasks` | `list`, `get`, `plan_day` | read |
| | `create`, `update`, `complete`, `postpone`, `link` | write_local |
| | `delete` | delete |
| `notes` | `search`, `get`, `backlinks` | read |
| | `create`, `edit`, `append`, `link` | write_local |
| | `delete` | delete |
| `files` | `search`, `read`, `preview`, `list` (chosen folders only) | read |
| | `write_artifact`, `create_doc`, `create_sheet`, `create_deck`, `combine` | write_local |
| | `share` (link outside the device) | publish_public |
| `people` | `find`, `get`, `timeline`, `relationships`, `needs_follow_up` | read |
| | `create`, `merge`, `note`, `set_reminder` | write_local |
| `reader` | `search`, `get`, `highlights` | read |
| | `save`, `highlight`, `brief` | write_local |
| `web` | `search`, `fetch` (Anthropic server tools via the gateway), `read_page` | read |
| `team` | `channels`, `history`, `mentions`, `search` | read |
| | `send_message`, `reply_in_thread` | send_external |
| `finance` | `accounts`, `transactions`, `spending`, `budgets` | read |
| | `categorize`, `set_budget`, `import_csv` | write_local |
| `health` | `series`, `summary`, `baseline` | read |
| | `log`, `import_export` | write_local |
| `places` | `geocode`, `route`, `weather`, `timezone` | read |
| `memory` | `recall`, `entity`, `episodes` | read |
| | `propose` (stored as `assumed`) | write_local |
| `ui` | `present`, `progress`, `ask_user`, `propose_proactive` | — (kernel-internal) |
| `os` | `delegate`, `schedule_trigger`, `create_mission`, `state_get/set` | — (gated by policy) |

## 3. Agents

Each entry: mission · capabilities · deterministic tools in the agent · skills · triggers and
missions · surfaces · without integrations.

### Mother (in the kernel)
- **Mission:** understand the person, decide who acts, keep agents honest, speak with one voice.
- **Capabilities:** intake (grammar → router → planner), delegation, arbitration, synthesis,
  missions, attention management, explanations, onboarding compiler (custom instructions → profile).
- **Skills:** `planning`, `arbitration`, `synthesis-voice` (persona from profile), `explain-why`,
  `profile-compiler`, `daily-briefing`.
- **Triggers:** briefing routine (profile, default 07:30); end-of-day wrap-up (opt-in).
- **Surfaces:** Today, Plan, Synthesis, Question (one question), Briefing.
- **Without a model key:** grammar and router still dispatch direct verbs; free text says
  "I'm offline — you can still use your apps."

### Calendar
- **Mission:** make time work: agenda, scheduling with others, conflicts, preparation.
- **Capabilities:** summarize a day or week; find time (with people, rules, time zones); schedule,
  reschedule, cancel; prepare for a meeting (pulls People, Mail, Notes); detect conflicts and
  overload; propose focus blocks.
- **Deterministic tools:** `find_free_time` (interval maths, working hours, rules, buffers),
  `rank_slots`, `timezone_convert`, `conflicts`.
- **Skills:** `scheduling-etiquette`, `meeting-prep`, `focus-protection`.
- **Triggers:** 06:00 agenda; on change → conflicts; 30 min before a meeting → prep card.
- **Surfaces:** Agenda, EventCard, SlotPicker, Conflict, MeetingPrep, Confirm (invites).
- **Without integrations:** a full local calendar; `.ics` import/export; invites become `.ics` drafts.
- **Driver work:** mcp-calendar needs attendees on `create_event` and runtime token refresh; a
  CalDAV driver is new.

### Mail
- **Mission:** keep the inbox under control without losing anything important.
- **Capabilities:** triage (needs reply, FYI, newsletters, receipts), summarize threads, draft in
  the person's voice, follow-ups, unsubscribe suggestions, extract tasks and events.
- **Deterministic tools:** sender reputation from history, thread dedupe, newsletter detection,
  reply-latency stats.
- **Skills:** `triage`, `reply-in-my-voice` (learns from sent mail, stored as procedural memory),
  `follow-up`, `extract-actions`.
- **Triggers and missions:** new mail → triage; "keep my inbox triaged every morning" mission;
  unanswered after N days → follow-up offer.
- **Surfaces:** Triage, ThreadSummary, MessageDraft, Confirm (send), FollowUps.
- **Without integrations:** import `.mbox`/`.eml` for search and drafting; sending needs an account.
- **Driver work:** mcp-email token refresh at runtime; tracing to stderr.

### Tasks
- **Mission:** a trustworthy list of what to do, planned against real time.
- **Capabilities:** capture from anything (drag an email, a note line, a message), plan a day
  against the calendar, weekly review, overdue nudges, break down projects.
- **Deterministic tools:** `plan_day` (existing, domain-scoped after #26), priority scoring, due-date parsing.
- **Skills:** `capture`, `plan-day`, `weekly-review`.
- **Triggers:** morning plan (with the briefing), overdue sweep, Friday review (opt-in).
- **Surfaces:** TaskList, DayPlan, Capture, Review.

### Notes
- **Mission:** a second brain the person actually writes in.
- **Capabilities:** create and edit markdown notes, meeting notes from calendar context, link
  related notes, summarize, answer from notes with citations.
- **Deterministic tools:** backlink graph, heading outline, markdown diff.
- **Skills:** `meeting-notes`, `summarize`, `link-ideas`.
- **Triggers:** after a meeting ends → offer a notes page prefilled with attendees and agenda.
- **Surfaces:** Note, Outline, Diff (proposed edit), Citations.

### Files & Docs
- **Mission:** find anything in the person's files and produce real documents.
- **Capabilities:** semantic search over chosen folders, previews, create documents, spreadsheets
  and decks (the proven deck workflow), combine artifacts (the real "fuse"), convert to PDF.
- **Deterministic tools:** file indexing and extraction, docx/xlsx/pptx/pdf engines (docx-mcp,
  worksheet-mcp, mcp-slides, mcp-pdf as local drivers).
- **Skills:** `deck-builder`, `report-writer`, `spreadsheet-model`, `combine-artifacts`.
- **Surfaces:** FileChip, Preview, Artifact (with open / reveal / share), Progress.
- **Without integrations:** fully local.

### People
- **Mission:** remember the people in the person's life and help keep in touch.
- **Capabilities:** resolve "Priya" across mail, calendar, notes and chat; relationship timeline;
  follow-up reminders (the Phase 2 tracker); birthdays and commitments; intro etiquette.
- **Deterministic tools:** entity resolution (aliases, emails, handles), interaction statistics,
  "last contacted".
- **Skills:** `keep-in-touch`, `introductions`.
- **Triggers:** weekly "haven't replied" sweep; birthday reminders from known dates.
- **Surfaces:** PersonCard, Timeline, FollowUps.

### Research
- **Mission:** answer questions from the web and the person's knowledge, with sources.
- **Capabilities:** research briefs with citations, topic watches (missions), reading lists,
  summaries read aloud, source evaluation.
- **Tools:** `web.search` / `web.fetch` (Anthropic server tools through the gateway — works with
  only the Anthropic key), `reader.*`, `notes.search`, mcp-news (keyless), mcp-browser `read_page`.
- **Skills:** `research-brief`, `source-evaluation`, `topic-watch`.
- **Missions:** "watch X and brief me weekly".
- **Surfaces:** Brief (with Citation), ReadingList, Observation.
- **Safety:** web content is untrusted; anything effectful downstream requires approval (taint, AGENTIC_OS §8.2).

### Team
- **Mission:** catch up on team chat and reply quickly.
- **Capabilities:** mention digest, channel catch-up, reply drafting, "what did I miss".
- **Tools:** mcp-slack (read; `send_message` as send_external).
- **Surfaces:** ChannelDigest, MessageDraft, Confirm.
- **Without integrations:** not offered (the Store shows "connect Slack").

### Finance
- **Mission:** know where the money goes; never move it.
- **Capabilities:** spending summaries, budgets, anomalies, subscriptions, upcoming bills.
- **Tools:** `finance.*`; mcp-banking read tools only. `initiate_payment` and every `financial`
  effect are never allowlisted (concept principle 10).
- **Surfaces:** Spending, Budget, Observation.
- **Without integrations:** CSV import.

### Health & Wellbeing
- **Mission:** organize health information and notice changes — never diagnose.
- **Capabilities:** sleep, activity and readings trends against the personal baseline; reminders;
  escalation language when a reading crosses a declared threshold.
- **Tools:** `health.*`; the Phase 2 diagnosis lint stays.
- **Surfaces:** Trend, Observation (neutral language: number, period, baseline).

### Travel & Places
- **Mission:** get the person where they are going.
- **Capabilities:** trip plans written into Calendar, routes and travel time for events, weather
  for the day's places, time-zone help.
- **Tools:** `places.*` (mcp-maps, mcp-weather, keyless), `calendar.*`, `web.search`.
- **Surfaces:** Itinerary, Map, Weather.
- **Note:** no flight or hotel booking in 1.0 (no driver exists); the agent says so.

### Insights
- **Mission:** help the person understand their own patterns.
- **Capabilities:** the Phase 2 intelligence layer, finally surfaced: daily patterns, the personal
  baseline, drift ("is this your normal?"), work/home balance, reading and focus trends.
- **Tools:** the content-free ledger and app aggregates through the Mother's scoped API
  (`src/intelligence/*`).
- **Triggers:** nightly baseline; observations wait for the next briefing (no interruptions).
- **Surfaces:** Observation (with "not now", "this is normal for me", "help me review").

### Automations (system)
- **Mission:** turn repeated approvals into explicit recipes the person controls.
- **Capabilities:** detect repetition in the audit log, propose recipes, run approved recipes,
  show every run, undo where possible.
- **Surfaces:** RecipeProposal, RecipeEditor, RunLog.

### Studio (system)
- **Mission:** make the OS look and feel the way the person asked.
- **Capabilities:** compile appearance instructions into design tokens, generate wallpapers
  (local image generation when available, gradients otherwise), adjust density and motion.
- **Surfaces:** ThemePreview, Choice.

## 4. Companion app data (sketch)

| App | Core tables (`app_<name>.*`) | Object kinds in the index |
|---|---|---|
| Calendar | `calendars`, `events`, `attendees`, `rules`, `sync_state` | event |
| Mail | `accounts`, `threads`, `messages`, `drafts`, `labels`, `sync_state` | thread, message, draft |
| Tasks | `lists`, `tasks`, `links` (existing `tasks` table migrates) | task |
| Notes | `notes`, `links`, `attachments` | note, chunk |
| Files | `roots`, `files`, `extracts`, `artifacts` | file, chunk, artifact |
| People | `people`, `identities`, `relations`, `interactions` | person, organisation |
| Reader | `pages`, `highlights`, `briefs` | page, chunk, brief |
| Finance | `accounts`, `transactions`, `budgets`, `categories` | transaction (aggregates only in memory) |
| Health | `series`, `readings`, `thresholds` | series (aggregates only) |

Every table carries `domain` (ADR-002), `source` (`local` or a driver id) and `source_id` for
idempotent sync; sensitive columns use the memory crypto.
