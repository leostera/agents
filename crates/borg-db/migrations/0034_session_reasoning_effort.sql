ALTER TABLE sessions
ADD COLUMN current_reasoning_effort TEXT;

ALTER TABLE session_messages
ADD COLUMN reasoning_effort TEXT;
