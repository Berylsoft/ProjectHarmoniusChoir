SELECT EXISTS (
    SELECT 1
        FROM users_latest
        WHERE wechat_openid = ?
);
