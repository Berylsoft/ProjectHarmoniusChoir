SELECT j.id, jf.id IS NOT NULL
    FROM bundle_jobs j
    LEFT JOIN bundle_jobs_finished jf
        ON jf.job_id = j.id
    WHERE j.project_id = ?;
