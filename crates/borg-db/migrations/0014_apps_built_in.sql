ALTER TABLE apps ADD COLUMN built_in INTEGER NOT NULL DEFAULT 0;

UPDATE apps
SET built_in = 1
WHERE app_id IN (
  'borg:app:codemode-runtime',
  'borg:app:memory-system'
);
