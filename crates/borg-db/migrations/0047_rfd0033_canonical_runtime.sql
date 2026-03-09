-- Phase 2a, 2b, 2c: New migration for canonical tables
-- RFD0033 Alignment

PRAGMA foreign_keys = OFF;

-- 1. MESSAGES
DROP TABLE IF EXISTS messages;
CREATE TABLE messages (
  message_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  sender_id TEXT NOT NULL,
  receiver_id TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  conversation_id TEXT NULL,
  in_reply_to_message_id TEXT NULL,
  correlation_id TEXT NULL,
  delivered_at TEXT NOT NULL,
  processing_state TEXT NOT NULL CHECK (processing_state IN ('pending', 'processed', 'failed')),
  processed_at TEXT NULL,
  failed_at TEXT NULL,
  failure_code TEXT NULL,
  failure_message TEXT NULL,
  CHECK (
    (processing_state = 'pending' AND processed_at IS NULL AND failed_at IS NULL) OR
    (processing_state = 'processed' AND processed_at IS NOT NULL AND failed_at IS NULL) OR
    (processing_state = 'failed' AND processed_at IS NULL AND failed_at IS NOT NULL)
  )
);

-- 2. TOOL CALLS
DROP TABLE IF EXISTS tool_calls;
CREATE TABLE tool_calls (
  tool_call_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  request_json TEXT NOT NULL,
  result_json TEXT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT NULL,
  error_code TEXT NULL,
  error_message TEXT NULL
);

-- 3. LLM CALLS
DROP TABLE IF EXISTS llm_calls;
CREATE TABLE llm_calls (
  llm_call_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  model TEXT NOT NULL,
  request_json TEXT NOT NULL,
  response_json TEXT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT NULL,
  error_code TEXT NULL,
  error_message TEXT NULL
);

-- 4. ACTORS
DROP TABLE IF EXISTS actors;
CREATE TABLE actors (
  actor_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  name TEXT NOT NULL,
  system_prompt TEXT NOT NULL,
  actor_prompt TEXT NOT NULL,
  default_provider_id TEXT NULL,
  model TEXT NULL,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- 5. PORTS
DROP TABLE IF EXISTS ports;
CREATE TABLE ports (
  port_id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  port_name TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  allows_guests INTEGER NOT NULL,
  assigned_actor_id TEXT NULL,
  settings_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- 6. PORT BINDINGS
DROP TABLE IF EXISTS port_bindings;
CREATE TABLE port_bindings (
  workspace_id TEXT NOT NULL,
  port_id TEXT NOT NULL,
  conversation_key TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (port_id, conversation_key)
);

-- INDICES
CREATE INDEX idx_messages_receiver_state_delivered ON messages (receiver_id, processing_state, delivered_at);
CREATE INDEX idx_messages_sender_delivered ON messages (sender_id, delivered_at);
CREATE INDEX idx_messages_conversation_delivered ON messages (conversation_id, delivered_at);
CREATE INDEX idx_messages_correlation ON messages (correlation_id);
CREATE INDEX idx_messages_in_reply_to ON messages (in_reply_to_message_id);

CREATE INDEX idx_tool_calls_actor_started ON tool_calls (actor_id, started_at);
CREATE INDEX idx_tool_calls_message ON tool_calls (message_id);
CREATE INDEX idx_tool_calls_status_started ON tool_calls (status, started_at);

CREATE INDEX idx_llm_calls_actor_started ON llm_calls (actor_id, started_at);
CREATE INDEX idx_llm_calls_message ON llm_calls (message_id);
CREATE INDEX idx_llm_calls_provider_model_started ON llm_calls (provider_id, model, started_at);

CREATE INDEX idx_actors_workspace ON actors (workspace_id);
CREATE INDEX idx_ports_workspace ON ports (workspace_id);
CREATE INDEX idx_port_bindings_actor ON port_bindings (actor_id);

PRAGMA foreign_keys = ON;
