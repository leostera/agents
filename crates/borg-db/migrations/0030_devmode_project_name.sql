ALTER TABLE devmode_projects
ADD COLUMN name TEXT NOT NULL DEFAULT '';

UPDATE devmode_projects
SET name = CASE
  WHEN trim(name) <> '' THEN trim(name)
  WHEN trim(description) <> '' THEN trim(description)
  ELSE trim(root_path)
END;
