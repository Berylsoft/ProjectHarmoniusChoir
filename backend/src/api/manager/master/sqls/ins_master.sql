INSERT
    INTO status_masters
        ( project_user_id, manager_id, created_at, comment )
    VALUES
        ( ?,               ?,          ?,          ? )
    RETURNING id;
