SELECT EXISTS(
    SELECT 1
        FROM deleted_files 
        WHERE file_id = ?
);
