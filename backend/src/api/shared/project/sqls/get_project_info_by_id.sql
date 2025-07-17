SELECT
    name,
    entry_question,
    require_harmony_group_intention,
    pre_submit_file_size_min,
    pre_submit_file_size_max,
    submit_file_size_min,
    submit_file_size_max,
    end_time
FROM projects
WHERE id = ?;
