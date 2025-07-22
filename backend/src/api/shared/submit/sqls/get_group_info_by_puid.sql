SELECT
    r.lead as lead,
    r.choir as choir,
    r.harmony as harmony
    FROM status_pre_submits s
    LEFT JOIN review_pre_submits r
        ON r.pre_submit_id = s.id
    WHERE s.project_user_id = ?
    ORDER BY s.id DESC
    LIMIT 1;
