INSERT INTO managers
      ( id, revision,     password, totp_secret, token_id, file_capacity, name )
    SELECT
        id, revision + 1, password, ?,           token_id, file_capacity, name
        FROM managers_latest
        WHERE id = ?
;
