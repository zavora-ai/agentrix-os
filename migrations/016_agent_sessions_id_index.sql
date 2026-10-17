-- PgSessionService::append_event (id-only addressing) looks sessions up by id, which the
-- (app_name, user_id, id) primary key cannot serve; append_event_for_identity uses the full key.
CREATE INDEX IF NOT EXISTS idx_agent_sessions_id ON agent_sessions (id);
