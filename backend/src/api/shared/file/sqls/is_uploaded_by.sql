SELECT EXISTS (
    SELECT 1
        FROM files
        WHERE id = ?
            AND project_id = ?
            AND user_id = ?
            AND (
                (? IS NULL AND manager_id IS NULL)
                OR (manager_id = ?)
            )
            AND stage = ?
);
