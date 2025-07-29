SELECT id, user_id
    FROM project_users
    WHERE project_id = ?
    ORDER BY id ASC;
