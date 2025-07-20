SELECT
    s.id as id,
    s.created_at as created_at,
    s.comment as comment,
    r.status as r_status, -- null-able, when not reviewed yet
    r.reason as r_reason, -- null-able
    r.reason_detail as r_reason_detail, -- null-able
    r.checked_file_group_id as r_checked_file_group_id
FROM status_submits s
LEFT JOIN review_submits r
    ON r.submit_id = s.id
WHERE s.project_user_id = ?
ORDER BY s.id DESC;
