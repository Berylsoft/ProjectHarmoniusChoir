SELECT id, project_id, manager_id
    FROM bundle_jobs j
    WHERE NOT EXISTS (
        SELECT 1
            FROM bundle_jobs_finished jf
            WHERE jf.job_id = j.id
    );

