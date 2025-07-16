SELECT name, s3_key
    FROM files
    WHERE id = ?
        AND project_id = ?;
