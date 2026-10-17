# ADR-007 — Agents are separately compiled processes

**Status:** Accepted (product owner, 2026-10-17) · **Date:** 2026-10-17 · **Milestone:** OS0 · **Concept:** `docs/AGENTIC_OS.md` §2.2, §3

## Context

Phase 1 and 2 "agents" were `LlmAgent` instances and scripted scenario workflows inside one server
process. They could not be installed, versioned, isolated or restarted on their own, and a single
fault took the whole OS down. The product promise is that agents are the new apps.

## Decision

Every world agent is its own binary (`agentrix-agent-<id>`) built with `agentrix-sdk` on adk-rust,
described by an `agent.toml` manifest, and run by the kernel's Agent Manager as a supervised process:

- **Work in:** A2A v1 tasks on a loopback port with a per-launch bearer token.
- **Everything else out:** MCP syscalls to the kernel over the process's stdio.
- **Models:** through the kernel's model gateway (ADR-008); agents never hold an API key.
- One process per agent, many concurrent tasks; lazy start, warm pool, idle stop, back-off restart.
- The Mother runs inside the kernel behind the same `Agent` trait.

## Alternatives considered

- **In-process crates** registered at compile time: fastest and simplest, but not installable,
  not isolated and not independently updatable — apps in name only.
- **WebAssembly components**: the strongest sandbox and portable, but adk-rust, tokio networking and
  the MCP stack do not run there today; revisit for untrusted third-party agents.

## Consequences

- An agent crash or hang is contained and recovered; the Activity monitor can show and stop it.
- The Store can install, update and remove agents like apps (OS4).
- We own a supervisor and must close A2A v1 gaps upstream (continuity, `input-required`,
  cancellation, durable task store) — AGENTIC_OS §12.3.
- Agent behaviour stays testable without processes through the SDK's in-process host.
