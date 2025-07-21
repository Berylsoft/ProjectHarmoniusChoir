SELECT EXISTS (
    SELECT 1
        FROM bundle_job_files
        WHERE file_id = ?
);
