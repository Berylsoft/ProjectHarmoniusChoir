CREATE INDEX IF NOT EXISTS index_users_is_deleted ON users(is_deleted);

CREATE INDEX IF NOT EXISTS index_users_id__revision ON users(id, revision DESC);
