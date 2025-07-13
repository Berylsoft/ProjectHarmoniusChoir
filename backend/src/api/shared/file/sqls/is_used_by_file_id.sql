SELECT EXISTS(
    SELECT 1
        FROM file_infos
        WHERE file_id = ?
);
