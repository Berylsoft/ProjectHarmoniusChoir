SELECT f.id, f.name
FROM file_infos fi
LEFT JOIN files f
    ON f.id = fi.file_id
WHERE
    fi.target_id = ?
    AND fi.target_type = 'Submit'
ORDER BY fi.id DESC;
