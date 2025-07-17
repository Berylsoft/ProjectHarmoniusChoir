INSERT
    INTO files
        ( project_id, user_id, manager_id, stage, name, s3_key, size, md5, content_type )
    VALUES
        ( ?,          ?,       ?,          ?,     ?,    ?,      ?,    ?,   ?)
    RETURNING id
