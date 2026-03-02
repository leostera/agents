ALTER TABLE port_bindings
ADD COLUMN actor_id TEXT;

CREATE INDEX IF NOT EXISTS idx_port_bindings_port_actor_id
ON port_bindings(port, actor_id);
