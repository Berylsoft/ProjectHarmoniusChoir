SELECT
    s.id as id,
    s.created_at as created_at,
    s.name as name,
    s.harmony_group_intention as harmony_group_intention, -- null-able
    s.comment as comment,
    f.id as f_id,
    f.name as f_name,
    r.manager_id as r_manager_id,
    r.status as r_status, -- null-able, when not reviewed yet
    r.lead as r_lead, -- null-able
    r.choir as r_choir, -- null-able
    r.harmony as r_harmony, -- null-able
    r.choir_harmony as r_choir_harmony, -- null-able
    r.reason as r_reason -- null-able
FROM status_pre_submits s
LEFT JOIN file_infos fi
    ON fi.target_id = s.id
    AND fi.target_type = 'PreSubmit'
LEFT JOIN files f
    ON f.id = fi.file_id
LEFT JOIN review_pre_submits r
    ON r.pre_submit_id = s.id
WHERE s.project_user_id = ?
ORDER BY s.id DESC;
