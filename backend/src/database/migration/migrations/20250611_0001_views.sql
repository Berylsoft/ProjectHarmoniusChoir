CREATE VIEW IF NOT EXISTS users_latest 
	AS
		SELECT id, revision, name, token_id, wechat_openid 
		FROM users AS usr
		WHERE is_deleted = false AND revision = (
			SELECT MAX(revision) 
			FROM users
			WHERE id = usr.id
		);
	;

CREATE VIEW IF NOT EXISTS users_latest_deleted
	AS
		SELECT id, revision, name, token_id, wechat_openid 
		FROM users AS usr
		WHERE is_deleted = true AND revision = (
			SELECT MAX(revision) 
			FROM users
			WHERE id = usr.id
		);
	;
