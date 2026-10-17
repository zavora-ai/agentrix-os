# Agentrix OS 1.0 — Delivery plan

**Status:** Proposed (2026-10-17) · Architecture [`AGENTIC_OS.md`](AGENTIC_OS.md) · Agents
[`BASE_AGENTS.md`](BASE_AGENTS.md) · Journeys J1–J13 in AGENTIC_OS §1.

Five milestones, `OS0`–`OS4`, about eleven weeks with the four existing lanes. Task ids are
`OSn-Tm` for commits and PRs (`feat(kernel): agent supervisor (OS0-T4)`). Every milestone ends
with a recorded demo of its journeys and green CI. Text, mouse and touch are tested in every
milestone; voice and camera are built to the same verb contract and verified once Google keys are
available.

| Lane | Owner | Scope |
|---|---|---|
| Platform & kernel | James | kernel, protocol, agent manager, broker, syscalls, gateway, gate, data layer, CI |
| Agents & worlds | Kim | SDK ergonomics, base agents, skills, evals, integrations drivers |
| Shell & surfaces | Robert | Svelte shell, Electron app, design system, surfaces renderer, VerbUI, onboarding UI |
| Intelligence & memory | Jotham | memory v2, embeddings, retrieval, consolidation, Insights, attention manager |

## OS0 — Foundations (weeks 1–2)

Goal: one real agent, as its own process, doing real work end to end through the new kernel and shell.

| Task | Lane | Deliverable |
|---|---|---|
| OS0-T1 | Platform | Accept the remaining ADRs (007, 008, 011 and 012 accepted 2026-10-17); convert the repo to a Cargo workspace (`crates/`, `agents/`, `shell/`, `desktop/`); move to sqlx 0.9; pin adk-rust |
| OS0-T2 | Platform | `agentrix-protocol`: manifest, task, surface, verb, syscall and ProActive schemas; JSON Schema export → TypeScript types for the shell |
| OS0-T3 | Platform | Kernel skeleton `agentrixd`: config, Postgres + pgvector, migrations (018+), shell gateway with the shell bus, owner auth carried over |
| OS0-T4 | Platform | Agent Manager: registry, spawn under the sandbox profile, health, restart with back-off, idle stop |
| OS0-T5 | Platform | Task Broker on A2A v1 with lineage, cancellation and a Postgres task store (upstream fixes in adk-rust as needed) |
| OS0-T6 | Platform | Syscall server (MCP over stdio) with the gate ported from `src/permissions/*`; audit and ledger |
| OS0-T7 | Platform | Model gateway: Anthropic key, Fable 5.1 for the Mother and Opus 5.5 for agents (ADR-011), effort, caching, fallbacks, budgets, usage records; record/replay mode for tests |
| OS0-T8 | Agents | `agentrix-sdk`: `Agent` trait, `Cx`, `llm_loop`, `present`, `ask_user`, in-process test host |
| OS0-T9 | Agents | Tasks app (port `tools/tasks.rs`) + Tasks agent as `agentrix-agent-tasks` |
| OS0-T10 | Shell | Svelte 5 shell + Electron desktop (thin main process, kernel sidecar): Today, Command bar, Dock, Field with cards, Tasks window, surface renderer for the first eight components; design tokens v1 |
| OS0-T11 | Shell | Playwright harness: desktop and touch emulation, record/replay kernel |
| OS0-T12 | Memory | Object index table and indexer hook; local embedder behind the `Embedder` trait |

**Exit:** "add a task for tomorrow at 3 to call mum" typed → Tasks agent (separate process) →
task in the Tasks app → surface on the field → visible in the Tasks window; killing the agent
mid-task restarts it and the task completes; CI runs the journey on record/replay.

## OS1 — Core apps, agents and the coordinator (weeks 3–5)

| Task | Lane | Deliverable |
|---|---|---|
| OS1-T1 | Platform | Calendar, Mail, Notes, Files, People apps: schemas, APIs, syscalls with effects |
| OS1-T2 | Agents | Calendar, Mail, Notes, Files & Docs, People agents with skills and evals |
| OS1-T3 | Agents | Drivers: mcp-email and mcp-calendar fixed (attendees, token refresh) and wired as sync drivers; `agentrix integrations build` |
| OS1-T4 | Platform | Mother v2: verb grammar, capability router (local embeddings), planner with structured output, parallel delegation, synthesis with citations, arbitration port |
| OS1-T5 | Memory | Memory v2: five kinds, hybrid retrieval (BM25 + vector + trigram, RRF), entity resolution, context builder with stable ordering |
| OS1-T6 | Shell | VerbUI: verb catalogue, mouse, touch and keyboard mappings, drag-to-delegate, combine, undo; surfaces for Agenda, EventCard, SlotPicker, MessageDraft, Confirm, PersonCard, Preview |
| OS1-T7 | Shell | Trust Center v1: agents and modes (change per agent and tool), approvals inbox as Confirm surfaces, audit, pause / resume |
| OS1-T8 | Shell | Onboarding wizard v1 (steps 1–5, 7–8) |

**Exit:** J2, J3, J4, J6, J9, J11, J13 pass on record/replay and once against the live model.

## OS2 — Proactive, missions and autonomy (weeks 6–8)

| Task | Lane | Deliverable |
|---|---|---|
| OS2-T1 | Platform | Scheduler and missions on adk `DurableScheduler` + `DurableWorker` (Postgres), approval parking, budgets, kill switch |
| OS2-T2 | Memory | Attention manager: priorities, budgets, quiet hours, focus, feedback |
| OS2-T3 | Shell | ProActive lane, Activity monitor (tasks, missions, lineage, spend), Today briefing |
| OS2-T4 | Agents | Research (web search via gateway, Reader app), Team (Slack), Travel & Places agents |
| OS2-T5 | Agents | Automations: recipe proposals from the audit log, recipe runs, undo |
| OS2-T6 | Agents | Files & Docs combine (real fuse) and the deck workflow ported |
| OS2-T7 | Platform | Taint tracking for untrusted content; MCP 2026 `input_required` answered by the gate |

**Exit:** J5, J7, J8 pass; a kernel restart mid-mission resumes without duplicate effects.

## OS3 — Intelligence and personalization (weeks 9–10)

| Task | Lane | Deliverable |
|---|---|---|
| OS3-T1 | Memory | Insights agent: patterns, baseline, drift, balance surfaced as Observations in the briefing |
| OS3-T2 | Agents | Finance (CSV + read-only banking) and Health (exports) agents |
| OS3-T3 | Platform | Profile compiler: custom instructions → persona, rules (deterministic constraints), routines, appearance |
| OS3-T4 | Shell | Studio: generated themes with live preview; onboarding step 6 |
| OS3-T5 | Memory | Memory app: browse, confirm, correct, export, delete; nightly consolidation |

**Exit:** J1 and J10 pass; the 1.0 journey suite runs nightly against the live model.

## OS4 — Multimodal, platform and polish (weeks 11–12)

| Task | Lane | Deliverable |
|---|---|---|
| OS4-T1 | Shell | Voice: composed pipeline (local STT, Mother, local TTS, VAD) and the Live path, both emitting VerbIntents — built, verified when keys allow |
| OS4-T2 | Shell | Camera gestures as VerbIntents (local detector); Live vision optional |
| OS4-T3 | Platform | Agent Store: install, enable, update, remove; package signing; Developer agent as the first Store install (J12) |
| OS4-T4 | Shell | Accessibility pass (WCAG 2.2 AA, keyboard parity, screen reader, reduced motion), responsive phone layout |
| OS4-T5 | Platform | Performance pass against the latency budgets; resilience suite (kill agents, drop integrations, restart kernel) |
| OS4-T6 | All | Sample space for demos (seeded, clearly labeled); docs; beta build for macOS |

**Exit:** J1–J13 green; latency budgets met; a beta build installs on a clean Mac.

## Risks

| Risk | Mitigation |
|---|---|
| adk-rust churn (160 commits, 6 breaking in 30 days) | Pin by commit; one owner per upstream change; contract tests on the APIs we use |
| A2A v1 gaps are larger than estimated | The broker hides the transport; fall back to a thin Agentrix task protocol over the same loopback channel while upstream catches up |
| Cost at Opus 5.5 for heavy users | Gateway budgets from OS0; per-route effort; tiering offer with measured numbers in OS1 |
| Integration drivers have no tests | Each driver gets a contract test against a recorded fixture before it is enabled by default |
| Scope: thirteen agents and nine apps | Agents share the SDK, surfaces and syscalls; OS1 proves the pattern on five before OS2–OS3 add the rest |
| Shell rewrite loses the prototype's feel | Port the visual language first (tokens, materials, motion) and review it in OS0 before features |
