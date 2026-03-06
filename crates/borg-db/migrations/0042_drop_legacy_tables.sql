-- Hard-cut cleanup for RFD0028.
-- Remove legacy user/agent-era tables and normalize remaining schema naming.

ALTER TABLE ports RENAME COLUMN default_agent_id TO default_actor_id;

DROP TABLE IF EXISTS agent_specs;
DROP TABLE IF EXISTS users;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS session_messages;
DROP TABLE IF EXISTS session_message_cursors;
DROP TABLE IF EXISTS port_session_ctx;
DROP TABLE IF EXISTS actor_mailbox;
DROP TABLE IF EXISTS behaviors;
DROP TABLE IF EXISTS policies_use;
DROP TABLE IF EXISTS policies;
