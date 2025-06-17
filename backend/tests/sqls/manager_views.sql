.echo on
BEGIN;
INSERT INTO managers (
    id,
    revision,
    pswd_argon2,
    argon2_m,
    argon2_t,
    argon2_p,
    argon2_len,
    pswd_salt,
    totp_secret,
    token_id
)
VALUES
(1, 1, X'31323334', 1, 1, 1, 64, X'34', X'31313131', 1),
(1, 2, X'35363738', 1, 1, 1, 64, X'34', X'31313131', 1),
(1, 3, X'35363738', 1, 1, 1, 64, X'35', X'31313132', 1),

(2, 1, X'32333435', 1, 1, 1, 64, X'33', X'36373839', 1);

SELECT * FROM managers_latest;
SELECT * FROM managers;
ROLLBACK;
