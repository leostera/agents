ALTER TABLE providers ADD COLUMN provider_kind TEXT;

UPDATE providers
SET provider_kind = provider
WHERE provider_kind IS NULL OR TRIM(provider_kind) = '';
