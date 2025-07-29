SELECT EXISTS (
    SELECT 1
        FROM bundle_job_includes
        WHERE project_user_id = ?
);
