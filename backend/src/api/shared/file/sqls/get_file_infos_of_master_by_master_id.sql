SELECT f.id, f.name
    FROM status_masters m
    LEFT JOIN file_infos fi
        ON fi.target_id = m.id
        AND fi.target_type = 'Master'
    LEFT JOIN files f
        ON f.id = fi.file_id
    WHERE m.id = ?;
