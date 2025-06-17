INSERT INTO users (id, revision, is_deleted, name, token_id, wechat_openid)
    SELECT id, revision + 1, is_deleted, ?, token_id, wechat_openid FROM users_latest
        WHERE id = ?
;
