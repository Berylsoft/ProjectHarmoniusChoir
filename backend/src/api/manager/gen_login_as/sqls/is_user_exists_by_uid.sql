SELECT EXISTS (
    SELECT 1
        FROM users_latest
        WHERE id = ?
);
