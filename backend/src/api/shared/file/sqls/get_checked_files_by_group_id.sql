SELECT DISTINCT file_id
FROM checked_files
WHERE group_id = ?
ORDER BY file_id ASC;
