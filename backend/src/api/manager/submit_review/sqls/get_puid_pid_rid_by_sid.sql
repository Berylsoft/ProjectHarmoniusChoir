SELECT s.project_user_id, pu.project_id, pu.user_id, r.id
FROM status_submits s
LEFT JOIN project_users pu ON pu.id = s.project_user_id
LEFT JOIN review_submits r ON r.submit_id = s.id
WHERE s.id = ?;
