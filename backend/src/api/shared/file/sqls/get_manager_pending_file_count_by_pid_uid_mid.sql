SELECT COUNT(*)
    FROM files
    WHERE project_id = ?
        AND user_id = ?
        AND manager_id = ?
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
