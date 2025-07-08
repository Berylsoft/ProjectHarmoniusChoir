CREATE INDEX IF NOT EXISTS index_users_is_deleted ON users(is_deleted);

CREATE INDEX IF NOT EXISTS index_users_id__revision ON users(id, revision DESC);

CREATE INDEX IF NOT EXISTS index_managers_id__revision ON managers(id, revision DESC);

CREATE INDEX IF NOT EXISTS index_project_managers_project_id__manager_id 
	ON project_managers(project_id, manager_id);
