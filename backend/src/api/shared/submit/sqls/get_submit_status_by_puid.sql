SELECT
    r.status as status,
    r.reason as reason,
    r.reason_detail as reason_detail
    FROM status_submits s
    LEFT JOIN review_submits r
        ON r.submit_id = s.id
    WHERE s.project_user_id = ?
    ORDER BY s.id DESC
    LIMIT 1;
