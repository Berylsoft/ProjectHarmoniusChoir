SELECT EXISTS (
    SELECT 1
        FROM status_masters
        WHERE project_user_id = ?
);
