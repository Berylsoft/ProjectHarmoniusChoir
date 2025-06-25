INSERT INTO managers
      ( id, revision,     password, totp_secret, token_id )
    SELECT
        id, revision + 1, password, ?,           token_id
        FROM managers_latest
        WHERE id = ?
;
