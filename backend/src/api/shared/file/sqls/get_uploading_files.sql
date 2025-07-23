SELECT id, name
    FROM files
    WHERE (
            (? IS NULL AND manager_id IS NULL AND user_id = ?)
            OR (manager_id = ?)
        )
        AND NOT EXISTS (
            SELECT 1
                FROM pending_files
                WHERE file_id = files.id
        )
        AND NOT EXISTS (
            SELECT 1
                FROM deleted_files
                WHERE file_id = files.id
        );
