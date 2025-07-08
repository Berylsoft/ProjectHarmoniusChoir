CREATE VIEW IF NOT EXISTS users_latest 
	AS
		SELECT *
		FROM users AS usr
		WHERE is_deleted = false AND revision = (
			SELECT MAX(revision) 
			FROM users
			WHERE id = usr.id
		);
	;

CREATE VIEW IF NOT EXISTS users_latest_deleted
	AS
		SELECT *
		FROM users AS usr
		WHERE is_deleted = true AND revision = (
			SELECT MAX(revision) 
			FROM users
			WHERE id = usr.id
		);
	;

CREATE VIEW IF NOT EXISTS managers_latest 
	AS
		SELECT *
		FROM managers AS mgr
		WHERE revision = (
			SELECT MAX(revision) 
			FROM managers
			WHERE id = mgr.id
		);
	;

CREATE VIEW IF NOT EXISTS project_managers_latest
	AS
		SELECT *
		FROM project_managers as pmgr
		WHERE is_revoke = false AND id = (
			SELECT MAX(id)
			FROM project_managers
			WHERE project_id = pmgr.project_id 
				AND manager_id = pmgr.manager_id
		)
	;

CREATE VIEW IF NOT EXISTS project_managers_latest_revoked
	AS
		SELECT *
		FROM project_managers as pmgr
		WHERE is_revoke = true AND id = (
			SELECT MAX(id)
			FROM project_managers
			WHERE project_id = pmgr.project_id 
				AND manager_id = pmgr.manager_id
		)
	;
