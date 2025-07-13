SELECT COUNT(*)
    FROM files
    WHERE user_id = ? AND manager_id IS NULL
        AND NOT EXISTS (
            SELECT 1
                FROM file_infos
                WHERE file_id = files.id
        );
