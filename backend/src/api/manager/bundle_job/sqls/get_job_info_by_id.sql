SELECT j.project_id, jf.s3_key
    FROM bundle_jobs j
    LEFT JOIN bundle_jobs_finished jf
        ON jf.job_id = j.id
    WHERE j.id = ?;
