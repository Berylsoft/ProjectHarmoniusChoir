SELECT id, name 
    FROM projects 
    WHERE id in (
        SELECT project_id
            FROM project_managers_latest 
            WHERE manager_id = ?
    );
