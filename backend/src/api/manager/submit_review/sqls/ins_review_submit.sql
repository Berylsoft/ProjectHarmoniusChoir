INSERT INTO review_submits (
    submit_id,
    manager_id,
    review_at,
    checked_file_group_id,
    status,
    reason,
    reason_detail
) VALUES ( ?, ?, ?, ?, ?, ?, ? );
