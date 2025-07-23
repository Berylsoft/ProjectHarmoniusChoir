SELECT f.id, f.name
    FROM status_pre_submits ps
    LEFT JOIN file_infos fi
        ON fi.target_id = ps.id
        AND fi.target_type = 'PreSubmit'
    LEFT JOIN files f
        ON f.id = fi.file_id
    WHERE ps.project_user_id = ?
    ORDER BY ps.id DESC
    LIMIT 1;
