SELECT COALESCE(SUM(size), 0)
    FROM files
    WHERE manager_id = ?
        AND NOT EXISTS (
            SELECT 1
                FROM file_infos
                WHERE file_id = files.id
        );
