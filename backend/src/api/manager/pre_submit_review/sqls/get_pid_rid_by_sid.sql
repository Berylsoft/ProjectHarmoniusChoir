SELECT pu.project_id, pu.user_id, r.id
FROM status_pre_submits s
LEFT JOIN project_users pu ON pu.id = s.project_user_id
LEFT JOIN review_pre_submits r ON r.pre_submit_id = s.id
WHERE s.id = ?;
