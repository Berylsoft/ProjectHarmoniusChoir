.echo on
BEGIN;
INSERT INTO users 
    (id, revision, is_deleted, name,            token_id, file_capacity)
VALUES
    (1,  1,        false,      'user 1',        1,        null),
    (1,  2,        false,      'user 1 name 2', 1,        2000000000),
    (1,  3,        true,       'user 1 name 2', 1,        2000000000),

    (2,  1,        false,      'user 2',        1,        null),
    (2,  2,        false,      'user 2 name 2', 1,        null),
    (2,  3,        false,      'user 2 name 3', 1,        null),

    (3,  1,        false,      'user 3',        1,        null),

    (4,  1,        true,       'user 4',        1,        null);

SELECT * FROM users_latest;
SELECT * FROM users_latest_deleted;
SELECT * FROM users;
ROLLBACK;
