INSERT
INTO status_submits
    ( project_user_id, created_at, comment )
VALUES
    ( ?,               ?,          ? )
RETURNING id;
