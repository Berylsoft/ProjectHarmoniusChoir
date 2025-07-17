INSERT
    INTO status_pre_submits
        ( project_user_id, created_at, harmony_group_intention, comment )
    VALUES
        ( ?,               ?,          ?,                       ? )
    RETURNING id
