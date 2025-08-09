SELECT id, name
    FROM files
    WHERE project_id = ?
        AND user_id = ?
        AND (
            (? IS NULL AND manager_id IS NULL)
            OR (manager_id = ?)
        )
        AND stage = ?
        AND EXISTS (
            SELECT 1
                FROM pending_files
                WHERE file_id = files.id
        )
        AND NOT EXISTS (
            SELECT 1
                FROM file_infos
                WHERE file_id = files.id
        )
        AND NOT EXISTS (
            SELECT 1
                FROM deleted_files
                WHERE file_id = files.id
        );
