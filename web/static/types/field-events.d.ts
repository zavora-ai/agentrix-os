/**
 * TypeScript types for the server → client contract (BK-110).
 *
 * Derived by hand from the Rust source of truth:
 *   - `src/events/sse.rs`        (FieldEvent — the SSE stream)
 *   - `src/domain.rs`            (Domain, ADR-002)
 *   - `src/permissions/*.rs`     (Mode, PendingAction, AgentPermissionView)
 *   - `src/state.rs`             (ChatTurn)
 *   - `src/routes/events.rs`     (UiEvent — the content-free client → server signals)
 *
 * Keep in sync with `SPECIFICATION.md` §7.5 when events change. True codegen
 * (e.g. ts-rs derives on FieldEvent) needs changes in src/events/sse.rs, which is
 * platform-lane owned — tracked under BK-110.
 */

/** Life domain of an agent, card, event or memory item (ADR-002). */
export type Domain = 'work' | 'home' | 'shared';

/** Agent authority mode (ADR-003). */
export type Mode = 'observe' | 'suggest' | 'automate';

/** Tool effect classes enforced by the permission gate (ADR-003). */
export type Effect =
  | 'read'
  | 'write_local'
  | 'schedule_with_others'
  | 'send_external'
  | 'publish_public'
  | 'financial'
  | 'delete';

export interface ConductStep {
  op: string;
  source?: string;
  target?: string;
  delay_ms?: number;
}

/** One SSE `data:` payload from intent / action / chat / a2a streams. */
export type FieldEvent =
  | { type: 'scenario'; key: string; text: string; total_cards: number }
  | { type: 'card_spawn'; index: number; card: CardSpec; domain: Domain }
  | { type: 'card_status'; index: number; status: string; line: string | null }
  | { type: 'card_resolve'; index: number; resolve: CardResolve }
  | { type: 'card_surface'; index: number; surface: string; slide: number; total: number }
  | { type: 'error'; message: string }
  | { type: 'suzy_summary'; key: string; html: string }
  | { type: 'suggest'; text: string; kind: string }
  | { type: 'conduct'; steps: ConductStep[] }
  | {
      type: 'deck_finish';
      big: string;
      sub: string;
      artifact_url: string | null;
      slide_count: number | null;
    }
  | {
      /** A tool call was queued for the user's approval (S2-T7). */
      type: 'permission_request';
      action_id: string;
      agent_id: string;
      domain: Domain;
      effect: string;
      summary: string;
      expires_at: string;
    }
  | {
      /** A pending action was resolved (approved / rejected / failed / expired). */
      type: 'action_result';
      action_id: string;
      status: string;
      audit_id?: string;
    }
  | { type: 'done' };

/** Card spec inside `card_spawn` — free-form JSON from the orchestrator; the
 *  fields the field UI reads. */
export interface CardSpec {
  glyph?: string;
  title?: string;
  agent?: string;
  attention?: boolean;
  surface?: string;
  waitsFor?: number;
  /** Injected client-side from the card_spawn envelope / CardRecord. */
  domain?: Domain;
  [key: string]: unknown;
}

export interface CardResolve {
  big?: string;
  sub?: string;
  lines?: string[];
  actions?: string[];
  artifact_url?: string;
  [key: string]: unknown;
}

/** GET/POST /api/sessions/{sid}/chat (S1-T5). */
export interface ChatTurn {
  role: 'user' | 'mother';
  text: string;
  ts: string;
}

export interface ChatHistoryResponse {
  session_id: string;
  turns: ChatTurn[];
}

/** GET /api/actions — one queued write awaiting approval (S2-T7). */
export interface PendingAction {
  id: string;
  user_id: string;
  session_id: string | null;
  agent_id: string;
  domain: Domain;
  tool: string;
  effect: Effect;
  /** Exact tool arguments to run on approval — content by nature. */
  args: unknown;
  /** Content-free description: agent, tool, effect, argument keys. */
  summary: string;
  status: 'pending' | 'approved' | 'rejected' | 'failed' | 'expired';
  trace_id: string | null;
  created_at: string;
  expires_at: string;
  resolved_at: string | null;
  result: unknown | null;
}

export interface ActionsResponse {
  user_id: string;
  actions: PendingAction[];
}

/** GET /api/permissions (S2-T8). */
export interface ToolView {
  name: string;
  effect: Effect | 'unclassified';
  mode: Mode;
}

export interface AgentPermissionView {
  agent_id: string;
  world: Domain;
  mode: Mode;
  default_mode: Mode;
  tool_overrides: Record<string, Mode>;
  tools: ToolView[];
}

export interface PermissionsResponse {
  user_id: string;
  agents: AgentPermissionView[];
  pause: unknown;
}

/** GET /api/audit — immutable record of executed, queued and denied effects (S2-T6). */
export interface AuditEntry {
  id: string;
  user_id: string;
  session_id: string | null;
  agent_id: string;
  domain: Domain;
  tool: string;
  effect: Effect;
  /** allowed | queued | approved | rejected | denied | failed | paused */
  decision: string;
  approval_id: string | null;
  recipe_id: string | null;
  mode: Mode;
  summary: string;
  undo_token: string | null;
  undone_at: string | null;
  trace_id: string | null;
  created_at: string;
}

export interface AuditResponse {
  user_id: string;
  entries: AuditEntry[];
}

/** POST /api/sessions/{sid}/events — content-free UI signals (S2-T2).
 *  Counts, domains and durations only; never content. */
export interface UiEvent {
  kind: 'ui_focus' | 'ui_blur' | 'ui_notification' | 'ui_card_open' | 'ui_lens';
  count?: number;
  domain?: Domain;
  duration_ms?: number;
}

export interface UiEventsBody {
  events: UiEvent[];
}
