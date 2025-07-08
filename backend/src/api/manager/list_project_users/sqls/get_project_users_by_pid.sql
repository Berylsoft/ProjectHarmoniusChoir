SELECT id, name 
    FROM project_users
    WHERE project_id = ?
    ORDER BY id ASC;
