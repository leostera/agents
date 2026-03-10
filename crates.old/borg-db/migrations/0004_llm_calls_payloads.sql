ALTER TABLE llm_calls ADD COLUMN request_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE llm_calls ADD COLUMN response_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE llm_calls ADD COLUMN response_body TEXT NOT NULL DEFAULT '';
