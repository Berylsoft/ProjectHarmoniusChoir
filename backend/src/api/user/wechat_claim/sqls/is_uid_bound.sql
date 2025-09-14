SELECT wechat_openid IS NOT NULL
    FROM users_latest
    WHERE uid = ?;
