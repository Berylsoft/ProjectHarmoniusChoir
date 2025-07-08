.echo on
BEGIN;
INSERT 
    INTO project_managers 
        ( id, project_id, manager_id, is_revoke )
    VALUES
        ( 1,  1,          1,          false ),
        ( 2,  2,          1,          false ),
        ( 3,  1,          2,          false ),

        ( 4,  1,          3,          false ),
        ( 5,  1,          3,          true ),

        ( 6,  1,          4,          false ),
        ( 7,  1,          4,          true ),
        ( 8,  1,          4,          false )
;

SELECT * FROM project_managers_latest;
SELECT * FROM project_managers_latest_revoked;
SELECT * FROM project_managers;
ROLLBACK;
