ALTER TABLE agent_specs ADD COLUMN name TEXT;

UPDATE agent_specs
SET name = CASE
    WHEN name IS NOT NULL AND trim(name) <> '' THEN name
    WHEN agent_id = 'borg:agent:default' THEN 'Default Agent'
    ELSE 'Agent'
END;

INSERT OR IGNORE INTO agent_specs(
    agent_id,
    name,
    model,
    system_prompt,
    tools_json,
    created_at,
    updated_at
)
VALUES (
    'borg:agent:default',
    'Default Agent',
    'gpt-4o-mini',
    'You are Borg''s default assistant.',
    '[]',
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
);
