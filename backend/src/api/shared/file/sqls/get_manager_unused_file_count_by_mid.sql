SELECT COUNT(*)
    FROM files
    LEFT JOIN deleted_files ON deleted_files.file_id = files.id
    LEFT JOIN pending_files ON pending_files.file_id = files.id
    LEFT JOIN file_infos ON file_infos.file_id = files.id
    WHERE files.manager_id = ?
        AND (
            deleted_files.id IS NOT NULL
            OR pending_files.id IS NULL
            OR file_infos.id IS NULL
        );
