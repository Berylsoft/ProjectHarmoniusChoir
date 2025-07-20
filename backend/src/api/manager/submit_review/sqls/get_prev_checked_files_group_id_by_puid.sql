SELECT MAX(r.checked_file_group_id)
    FROM status_submits s
    LEFT JOIN review_submits r
        ON r.submit_id = s.id
    WHERE s.project_user_id = ? AND r.checked_file_group_id IS NOT NULL;
