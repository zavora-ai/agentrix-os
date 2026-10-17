# ADR-011 — Anthropic models by default: Fable 5.1 for the Mother, Opus 5.5 for agents

**Status:** Accepted (product owner, 2026-10-17) · **Date:** 2026-10-17 · **Milestone:** OS0 · **Concept:** `docs/AGENTIC_OS.md` §11

## Context

Text agents defaulted to `claude-fable-5-1` (PR #19), and twenty-two agents broke because they sent
`temperature`, which Claude 5 models reject (fixed in #23). The 1.0 coordinator and agents need one
clear model policy that is applied in one place, the kernel's model gateway (ADR-008).

The coordinator's judgement (planning across worlds, arbitration, one coherent answer) is where
model capability pays off most; agents do narrower, tool-heavy work.

## Decision

| Role | Model | Effort |
|---|---|---|
| Mother: planning and arbitration | `claude-fable-5-1` ($10 / $50 per MTok) | medium |
| Mother: clarification and synthesis | `claude-fable-5-1` | low |
| Agents: interactive work | `claude-opus-5-5` ($4 / $20 per MTok) | medium |
| Agents: research and documents | `claude-opus-5-5` | high |
| Agents: background missions and triage | `claude-opus-5-5` | low |

- Cheaper tiers (`claude-sonnet-5-5`, `claude-haiku-5-5`) remain the **person's choice** per agent or for
  background work, offered with measured costs in the Trust Center.
- Routing avoids the model whenever it can: the verb grammar and the local-embedding capability
  router resolve direct commands before the Mother's planner is called (AGENTIC_OS §4.2).
- The gateway enforces Claude 5 rules centrally for both models: no sampling parameters; adaptive
  thinking (Fable 5.1 always thinks — the `thinking` parameter is omitted and depth is set with
  `effort`; Opus 5.5 cannot disable thinking and defaults to `medium`, so effort is always explicit);
  no forced `tool_choice` (`strict` tools and structured outputs instead); stable prompt prefixes
  with one-hour caching; append-only histories with server-side compaction, because thinking blocks
  are bound to the conversation that produced them; per-message effort changes through
  mid-conversation output config instead of editing the prompt; server-side refusal fallbacks
  (`fallbacks: "default"`); `thinking.display: "updates"` for progress notes; budgets per agent,
  mission and day.
- Voice stays on Gemini Live / OpenAI Realtime (Anthropic has no realtime voice API), with a composed
  local STT → Mother → local TTS pipeline as the default voice path. Embeddings are local.

## Consequences

- **Cost:** about $6 for a heavy day (40 Mother turns on Fable at ~$0.07, 60 agent turns and 50
  background steps on Opus at ~$0.03), versus ~$4.50 with Opus everywhere. Measured in OS1.
- **Latency:** Fable turns can run long on hard tasks. The fast path keeps simple commands off the
  model, Mother calls stream, and synthesis runs at `low` effort. If OS1 measurements miss the
  §11.3 budgets, synthesis moves to Opus 5.5 while planning stays on Fable.
- **Data retention:** Claude Fable 5.1 requires 30-day data retention and is not available under
  zero data retention unless Anthropic authorizes it. The Mother sees context from both worlds, so
  this is stated in onboarding's privacy step; a person who needs zero retention can switch the
  Mother to Opus 5.5 in the Trust Center.
- **Refusals:** both models can stop with `refusal`; the gateway's fallback routes the turn and the
  shell says plainly when a request was declined.
- Two model namespaces mean two prompt caches; each is kept warm by stable prefixes.
