SELECT id
    FROM project_managers_latest
    WHERE project_id = ? 
        AND manager_id = ?;
