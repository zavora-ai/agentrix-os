# Agentrix OS (spatial-os)

Agentic operating system prototype — spatial field UI driven by server-sent events.

## Quick start

```bash
cp .env.example .env
cargo run
```

Open [http://localhost:9847](http://localhost:9847). Use `?demo=1` for offline simulation (no server SSE).

## Layout

| Path | Purpose |
|------|---------|
| `web/` | Field UI (`index.html`, `static/field-client.js`) |
| `src/` | Axum server, SSE orchestration, AWP routes |
| `audio/` | Prerecorded voice clips |
| `scripts/` | Demo capture (`capture.js`) and TTS generation (`gen_audio.py`) |
| `docs/` | Specification, implementation plan, Phase 2 concept + sprint plan, ADRs |
| `demo/` | Generated demo assets (gitignored) |
| `deploy/` | Deployment configs (M11) |

## API

- `POST /api/sessions` → `{ session_id, user_id }`
- `POST /api/sessions/{id}/intent` → SSE stream (`scenario`, `card_*`, `suzy_summary`, `done`)
- `GET /artifacts/{session_id}/{file}` — deck artifacts (.xlsx, .docx, .pptx)
- `GET /health`
- `GET /.well-known/awp.json`, `GET /awp/manifest` — AWP discovery

Phase 2 · R1 (Personal AI OS foundation — see `docs/SPRINT_PLAN.md`):

- `POST /api/sessions/{id}/chat` → SSE via the Mother Agent (multi-world fan-out, one synthesis); `GET` returns the transcript
- `GET /api/actions` · `POST /api/actions/{id}/approve|reject|edit` · `POST /api/actions/approve` (batch) · `GET /api/audit`
- `GET/PUT /api/permissions` (observe | suggest | automate per agent, per-tool overrides) · `POST /api/pause` · `POST /api/resume`
- `GET/POST/DELETE /api/memory` · `PATCH/DELETE /api/memory/{id}` · `GET /api/memory/export` (known · assumed · recommended, with provenance)
- `POST /api/sessions/{id}/events` — content-free UI signals for the activity ledger

## Real deck workflow (M1)

Set `GOOGLE_API_KEY` in `.env` and build MCP servers:

```bash
(cd ../mcp-servers/worksheet-mcp && cargo build --release)
(cd ../mcp-servers/docx-mcp && cargo build --release)
(cd ../mcp-servers/mcp_slides && cargo build --release)
```

Without the API key, deck intents use mock SSE (M0 behavior).

## Validate

```bash
cargo test --test validate
```

Runs MCP boot, `gemini-3.1-flash-lite` smoke, agent wiring, and mock SSE checks. Full deck E2E (slow):

```bash
cargo test --test validate deck_workflow_writes_three_artifacts -- --ignored
```

## Docs

- [Specification](docs/SPECIFICATION.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Personal AI OS — concept & architecture (Phase 2)](docs/PERSONAL_AI_OS.md)
- [Personal AI OS — sprint plan (Phase 2)](docs/SPRINT_PLAN.md)
- [Personal AI OS — team progress (Phase 2, per-person sprint tables)](docs/PROGRESS.md)
- [Personal AI OS — interactive one-page artifact](docs/personal-ai-os.html) (open locally or via a raw HTML preview)
