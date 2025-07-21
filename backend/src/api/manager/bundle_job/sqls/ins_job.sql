INSERT
    INTO bundle_jobs
        ( project_id, manager_id, created_at )
    VALUES
        ( ?,          ?,          ? )
    RETURNING id;
