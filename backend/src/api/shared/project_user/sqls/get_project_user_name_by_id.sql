SELECT name
    FROM status_pre_submits
    WHERE project_user_id = ?
    ORDER BY id DESC
    LIMIT 1;
