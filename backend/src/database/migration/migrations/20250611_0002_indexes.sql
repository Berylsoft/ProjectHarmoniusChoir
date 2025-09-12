-- user/manager
CREATE INDEX IF NOT EXISTS idx_users__is_deleted
    ON users(is_deleted);

CREATE INDEX IF NOT EXISTS idx_users__id__revision
    ON users(id, revision DESC);

CREATE INDEX IF NOT EXISTS idx_managers__id__revision
    ON managers(id, revision DESC);

-- project user/manager
CREATE INDEX IF NOT EXISTS idx_project_users__project_id
    ON project_users(project_id);

CREATE INDEX IF NOT EXISTS idx_project_managers__project_id__manager_id
    ON project_managers(project_id, manager_id);

CREATE INDEX IF NOT EXISTS idx_project_managers__is_revoke
    ON project_managers(is_revoke);

-- bundle jobs
CREATE INDEX IF NOT EXISTS idx_bundle_jobs__project_id
    ON bundle_jobs(project_id);

CREATE INDEX IF NOT EXISTS idx_bundle_jobs_finished__job_id
    ON bundle_jobs_finished(job_id);

CREATE INDEX IF NOT EXISTS idx_bundle_job_includes__job_id
    ON bundle_job_includes(job_id);


-- file
CREATE INDEX IF NOT EXISTS idx_files__pid__uid__mid__stage
    ON files(project_id, user_id, manager_id, stage);

CREATE INDEX IF NOT EXISTS idx_files__mid
    ON files(manager_id);

CREATE INDEX IF NOT EXISTS idx_deleted_files__file_id
    ON deleted_files(file_id);

CREATE INDEX IF NOT EXISTS idx_pending_files__file_id
    ON pending_files(file_id);

CREATE INDEX IF NOT EXISTS idx_file_infos__file_id
    ON file_infos(file_id);

CREATE INDEX IF NOT EXISTS idx_file_infos__target_id__target_type
    ON file_infos(target_id, target_type);

CREATE INDEX IF NOT EXISTS idx_checked_files__group_id__file_id
    ON checked_files(group_id, file_id ASC);

-- status/review
CREATE INDEX IF NOT EXISTS idx_status_pre_submits__puid
    ON status_pre_submits(project_user_id);

CREATE INDEX IF NOT EXISTS idx_status_submits__puid
    ON status_submits(project_user_id);

CREATE INDEX IF NOT EXISTS idx_status_masters__puid
    ON status_masters(project_user_id);

CREATE INDEX IF NOT EXISTS idx_status_mixed__puid
    ON status_mixed(project_user_id);

CREATE INDEX IF NOT EXISTS idx_review_pre_submits__psid
    ON review_pre_submits(pre_submit_id);

CREATE INDEX IF NOT EXISTS idx_review_submits__psid
    ON review_submits(submit_id);

-- other
CREATE INDEX IF NOT EXISTS idx_nda_agreed__puid
    ON nda_agreed(project_user_id);
