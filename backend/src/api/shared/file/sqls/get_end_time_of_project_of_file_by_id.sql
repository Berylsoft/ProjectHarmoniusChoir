SELECT p.end_time
    FROM files f
    LEFT JOIN projects p
        ON p.id = f.project_id
    WHERE f.id = ?;
