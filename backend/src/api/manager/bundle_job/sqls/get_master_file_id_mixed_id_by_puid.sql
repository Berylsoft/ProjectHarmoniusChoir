SELECT fi.file_id, mi.id
    FROM status_masters m
    LEFT JOIN file_infos fi
        ON fi.target_id = m.id
        AND fi.target_type = 'Master'
    LEFT JOIN status_mixed mi
        ON mi.project_user_id = m.project_user_id
    WHERE m.project_user_id = ?;
