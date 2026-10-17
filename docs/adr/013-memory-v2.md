# ADR-013 — Memory v2: Postgres + pgvector, one object index, local embeddings, hybrid retrieval

**Status:** Proposed · **Date:** 2026-10-17 · **Milestone:** OS0–OS1 · **Concept:** `docs/AGENTIC_OS.md` §6

## Context

Phase 2 memory stores profile facts with kinds and provenance but has no semantic recall, no
documents and no entity graph. adk-memory's Postgres backend has vector search but its entries have
no id, kind or score, and neither adk-memory nor adk-rag ships a local embedding provider.

## Decision

- One Postgres 17 database per person with pgvector, pg_trgm and full-text search.
- Every companion-app object is mirrored into `mem.objects` with its ref, kind, domain, text,
  sensitivity and embedding — the single index for search, retrieval and citations.
- Five memory kinds: profile & facts (existing known / assumed / recommended), episodic, semantic
  knowledge, entities & relations, procedural rules.
- **Local embeddings** behind one `Embedder` trait (ONNX sentence model by default; EmbeddingGemma
  via adk-mistralrs as an option; Voyage as an optional hosted provider), versioned per row.
- **Hybrid retrieval:** BM25 + vector + trigram fused with reciprocal-rank fusion, domain and kind
  filters, entity expansion, recency decay; results carry refs and provenance.
- Agents propose (`assumed`); only the person makes things `known`; nightly consolidation; the
  Phase 2 encryption, domain isolation and content-free ledger rules stay.

## Consequences

- Universal search and grounded answers work across apps without per-agent indexes.
- We contribute a shared embedding trait and local adapters to adk-rust.
- Postgres becomes required for 1.0 (in line with ADR-006); there is no SQLite mode.
