.echo on
BEGIN;
INSERT INTO users 
    (id, revision, is_deleted, name,            token_id)
VALUES
    (0,  1,        false,      'user 1',        1),
    (0,  2,        false,      'user 1 name 2', 1),
    (0,  3,        true,       'user 1 name 2', 1),

    (1,  1,        false,      'user 2',        1),
    (1,  2,        false,      'user 2 name 2', 1),
    (1,  3,        false,      'user 2 name 3', 1),

    (2,  1,        false,      'user 3',        1),

    (3,  1,        true,       'user 4',        1);

SELECT * FROM users_latest;
SELECT * FROM users_latest_deleted;
SELECT * FROM users;
ROLLBACK;
