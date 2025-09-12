SELECT
    p.id as id,
    p.name as name,
    EXISTS (
        SELECT 1
            FROM project_users pu
            WHERE pu.user_id = ?
                AND pu.project_id = p.id
    ) as joined,
    p.end_time as end_time
    FROM projects p;
--# analyze ignore
