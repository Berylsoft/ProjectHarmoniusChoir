SELECT revision, password, totp_secret
    FROM managers_latest
    WHERE id = ?;
