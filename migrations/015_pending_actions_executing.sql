-- Approvals run once: an approval claims its action (pending → executing) with a conditional
-- UPDATE before running the tool, so a second approve or a racing reject finds it taken.
ALTER TABLE pending_actions DROP CONSTRAINT IF EXISTS pending_actions_status_check;
ALTER TABLE pending_actions ADD CONSTRAINT pending_actions_status_check
    CHECK (status IN ('pending', 'executing', 'approved', 'rejected', 'expired', 'failed'));
