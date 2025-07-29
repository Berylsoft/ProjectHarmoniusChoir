SELECT
    f.id as id,
    f.name as name,
    f.s3_key as s3_key
    FROM status_masters m
    LEFT JOIN file_infos fi
        ON fi.target_id = m.id
        AND fi.target_type = "Master"
    LEFT JOIN files f
        ON f.id = fi.file_id
    WHERE m.project_user_id = ?;
