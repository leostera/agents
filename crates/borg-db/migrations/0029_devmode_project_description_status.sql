ALTER TABLE devmode_projects
ADD COLUMN description TEXT NOT NULL DEFAULT '';

ALTER TABLE devmode_projects
ADD COLUMN status TEXT NOT NULL DEFAULT 'ONGOING';

UPDATE devmode_projects
SET status = CASE
  WHEN status IS NULL OR trim(status) = '' THEN 'ONGOING'
  ELSE upper(status)
END;
