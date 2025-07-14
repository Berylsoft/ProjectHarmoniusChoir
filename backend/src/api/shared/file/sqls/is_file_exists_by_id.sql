SELECT EXISTS (
    SELECT 1
        FROM files
        WHERE id = ?
);
