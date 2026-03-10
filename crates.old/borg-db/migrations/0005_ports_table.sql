CREATE TABLE IF NOT EXISTS ports (
    port_id TEXT PRIMARY KEY,
    port_name TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    allows_guests INTEGER NOT NULL DEFAULT 1,
    default_agent_id TEXT,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT OR IGNORE INTO ports (
    port_id,
    port_name,
    provider,
    enabled,
    allows_guests,
    default_agent_id,
    settings_json,
    created_at,
    updated_at
)
SELECT
    ('borg:port:' || dp.port_name) AS port_id,
    dp.port_name,
    COALESCE((
        SELECT ps.value
        FROM port_settings ps
        WHERE ps.port = dp.port_name AND ps.key IN ('provider', 'kind')
        ORDER BY ps.updated_at DESC
        LIMIT 1
    ), CASE WHEN dp.port_name = 'telegram' THEN 'telegram' ELSE 'custom' END) AS provider,
    COALESCE((
        SELECT CASE
            WHEN lower(trim(ps.value)) IN ('0','false','no','off') THEN 0
            ELSE 1
        END
        FROM port_settings ps
        WHERE ps.port = dp.port_name AND ps.key = 'enabled'
        ORDER BY ps.updated_at DESC
        LIMIT 1
    ), 1) AS enabled,
    COALESCE((
        SELECT CASE
            WHEN lower(trim(ps.value)) IN ('0','false','no','off') THEN 0
            ELSE 1
        END
        FROM port_settings ps
        WHERE ps.port = dp.port_name AND ps.key = 'allows_guests'
        ORDER BY ps.updated_at DESC
        LIMIT 1
    ), 1) AS allows_guests,
    (
        SELECT ps.value
        FROM port_settings ps
        WHERE ps.port = dp.port_name AND ps.key = 'default_agent_id'
        ORDER BY ps.updated_at DESC
        LIMIT 1
    ) AS default_agent_id,
    COALESCE((
        SELECT json_group_object(ps.key, ps.value)
        FROM port_settings ps
        WHERE ps.port = dp.port_name
          AND ps.key NOT IN ('provider', 'kind', 'enabled', 'allows_guests', 'default_agent_id')
    ), '{}') AS settings_json,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now') AS created_at,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now') AS updated_at
FROM (
    SELECT DISTINCT port AS port_name FROM port_settings
    UNION
    SELECT DISTINCT port AS port_name FROM port_bindings
    UNION
    SELECT DISTINCT port AS port_name FROM port_session_ctx
) dp;

DROP TABLE IF EXISTS port_settings;
