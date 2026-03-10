-- Rename TaskGraph identity columns to actor terminology.

ALTER TABLE taskgraph_tasks RENAME COLUMN assignee_agent_id TO assignee_actor_id;
ALTER TABLE taskgraph_tasks RENAME COLUMN reviewer_agent_id TO reviewer_actor_id;
