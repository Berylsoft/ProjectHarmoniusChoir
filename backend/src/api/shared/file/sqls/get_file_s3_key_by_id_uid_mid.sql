SELECT s3_key
FROM files
WHERE id = ?
    AND user_id = ?
    AND (
        (? IS NULL AND manager_id IS NULL)
        OR (manager_id = ?)
    );
