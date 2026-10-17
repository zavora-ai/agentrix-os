# ADR-008 — The kernel mediates every model call, tool call, data access and secret

**Status:** Accepted (product owner, with ADR-007, 2026-10-17) · **Date:** 2026-10-17 · **Milestone:** OS0 · **Concept:** `docs/AGENTIC_OS.md` §2.1, §8

## Context

Agents read untrusted content (mail, web pages) and act on the person's behalf. If each agent held
API keys, database handles and network access, one confused or injected agent could exfiltrate or
act without the gate seeing it. Integration servers built from `mcp-servers` `origin/main` also gate
their own write tools (MCP 2026 `input_required`), which would double every approval.

## Decision

- Agents hold **no secrets, no database handle and no network** beyond loopback to the kernel; the
  manifest's sandbox profile is enforced with adk-sandbox (Seatbelt on macOS, bubblewrap + seccomp on Linux).
- **Model gateway:** agents call an Anthropic-compatible endpoint on the kernel with a per-agent
  token; the kernel adds the key and applies policy (effort, caching, fallbacks, betas, budgets).
- **Syscalls:** all data, tools, memory, UI and peer requests are MCP tools served by the kernel,
  each classified by effect and decided by the permission gate (ADR-003), audited and ledgered.
- **One approval:** the kernel is the only approver; it answers an integration server's
  `input_required` prompt only after the gate approved that action.
- **Taint:** effects requested by a plan step shaped by untrusted content require approval even in Automate.

## Consequences

- A compromised agent can only *ask*; every effect is visible in the Trust Center.
- Budgets, usage, cache hit rates and refusals are measured in one place.
- The gateway hop adds well under a millisecond on loopback; streaming passes through.
- The SDK must point adk's Anthropic client at the gateway (base URL override; add upstream if missing).
