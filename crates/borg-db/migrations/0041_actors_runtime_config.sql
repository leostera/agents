-- Actor-owned runtime config (model/provider) replaces agent_specs lookups in runtime paths.

ALTER TABLE actors
ADD COLUMN model TEXT;

ALTER TABLE actors
ADD COLUMN default_provider_id TEXT;
