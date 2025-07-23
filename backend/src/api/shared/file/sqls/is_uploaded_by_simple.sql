SELECT EXISTS (
    SELECT 1
        FROM files
        WHERE id = ?
            AND (
                (? IS NULL AND manager_id IS NULL AND user_id = ?)
                OR (manager_id = ?)
            )
);
