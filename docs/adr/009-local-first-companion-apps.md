# ADR-009 — Local-first companion apps own the data; integrations are sync drivers

**Status:** Proposed · **Date:** 2026-10-17 · **Milestone:** OS1 · **Concept:** `docs/AGENTIC_OS.md` §5, §7

## Context

Without integrations the prototype streamed scripted mock data that read as the person's own. The
MCP inventory shows integrations are optional and uneven: no release binaries, calendar exits
without a token, and no local store exists for notes, files, contacts, tasks or a calendar.

## Decision

Each base agent owns a **companion app** whose data lives in the kernel's Postgres (`app_<name>.*`):
Calendar, Mail, Tasks, Notes, Files, People, Reader, Finance, Health, plus Today, Memory, Trust
Center, Activity, Store and Studio. People and agents manipulate the same objects, addressed by
stable refs (`agentrix://<app>/<kind>/<id>`). Integrations are **drivers** that sync external data
into the apps (idempotent, cursor-based) and execute outbound effects on request through the gate.

The live OS has **no mock data path**. Demos use a separate, clearly labeled sample space.

## Consequences

- The OS is useful on day one with no accounts connected, and honest by construction.
- We build nine small apps (schema, API, syscalls, shell module) — shared patterns keep each small.
- Search, memory and citations work uniformly through the object index (ADR-013).
- `mcp-email` and `mcp-calendar` need fixes before they are enabled by default (attendees, token refresh).
