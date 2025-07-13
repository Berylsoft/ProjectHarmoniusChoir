SELECT EXISTS(
    SELECT 1
        FROM pending_files
        WHERE file_id = ?
);
