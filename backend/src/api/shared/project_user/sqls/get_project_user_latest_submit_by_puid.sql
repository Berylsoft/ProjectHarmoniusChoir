SELECT id, created_at
    FROM status_submits
    WHERE id = (
        SELECT MAX(id)
        FROM status_submits
        WHERE project_user_id = ?
    );
