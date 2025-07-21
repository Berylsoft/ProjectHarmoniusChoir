SELECT manager_id, created_at, comment
    FROM status_masters
    WHERE project_user_id = ?;
