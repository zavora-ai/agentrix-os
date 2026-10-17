-- Sessions created by a signed-in user belong to that user: the anonymous session routes
-- (intent, chat, events, voice) refuse any other caller. Anonymous sessions stay open.
ALTER TABLE ui_sessions ADD COLUMN IF NOT EXISTS signed_in BOOLEAN NOT NULL DEFAULT FALSE;

-- Existing sessions: a signed-in session's user_id is the JWT subject, which is a users.id;
-- anonymous sessions get a random UUID that is never one.
UPDATE ui_sessions SET signed_in = TRUE
WHERE NOT signed_in AND user_id IN (SELECT id::text FROM users);
