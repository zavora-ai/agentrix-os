# ADR-010 — Declarative surfaces and one verb grammar across every input

**Status:** Proposed · **Date:** 2026-10-17 · **Milestone:** OS0–OS1 · **Concept:** `docs/AGENTIC_OS.md` §10.3–10.4

## Context

Prototype cards were ad-hoc JSON with resolvers that filled in defaults, rendered through
`innerHTML` (stored XSS), and their buttons showed "✓ Done" without doing anything. Each input
(text, drag, gesture, voice) had its own code path.

## Decision

- **Surfaces:** agents emit declarative component trees from the Agentrix catalogue (Card, Agenda,
  MessageDraft, Confirm, Observation …), A2UI-compatible, validated against JSON Schema, generated
  with Claude structured outputs, streamed and patched in place, rendered natively by the shell.
  No agent ever sends HTML.
- **VerbUI:** every interaction is a `VerbIntent {verb, objects, target?, args}`. Surfaces declare
  the verbs their objects accept. Text, mouse, touch, keyboard, voice and camera all produce
  VerbIntents; local verbs run in the shell with undo, effectful verbs go to the owning agent and
  through the gate. A button can only claim success when the agent returned a result.

## Consequences

- One contract to test (Playwright drives it with text, mouse and touch); voice and camera reuse it.
- The XSS class from the October review disappears by construction.
- The surface catalogue becomes a product asset; new components need a schema, a renderer and an
  accessibility review.
