.echo on
BEGIN;
INSERT INTO managers (
    id,
    revision,
    password,
    totp_secret,
    token_id,
    file_capacity,
    name
)
VALUES
(0, 1, "$argon2id$v=19$m=16384,t=2,p=1$qVCwm62QuWspang5MhiGyQ$Dahon8dFYEQztNSqgPCB/kwr/jvR4UXXHyAjHocC/+WFzV8lI4Sc0nYmqYP/wcOCymUWVeX5hfgQyFaDd0xUNA", X'31313131', 1, null, "mgr"),
(0, 2, "$argon2id$v=19$m=16384,t=2,p=1$It6mOT1+lI6GwQxp/M4vTw$9kWA2nlEqjMO9r6U+VLvg5dp54mtgTDWome01vKwxi8HgenvBrhUfJZgDgBZarIcHv4qBSnmy45TsSMW8bQ5vg", X'31313131', 1, 2000000000, "mgr"),
(0, 3, "$argon2id$v=19$m=16384,t=2,p=1$It6mOT1+lI6GwQxp/M4vTw$9kWA2nlEqjMO9r6U+VLvg5dp54mtgTDWome01vKwxi8HgenvBrhUfJZgDgBZarIcHv4qBSnmy45TsSMW8bQ5vg", X'31313132', 1, 2000000000, "mgr"),

(1, 1, "$argon2id$v=19$m=16384,t=2,p=1$OxX3WVG8ofcnSQaI2M64bw$QxS0lA5CiKCDk4AUIjtfi9/96vKPX2yXKwls8fLWV3UbhUzC7EFFYPDHtgFeyd+kA/FkGRvCdbUC0Vtw2qu2GA", X'36373839', 1, null, "mgr2");

SELECT * FROM managers_latest;
SELECT * FROM managers;
ROLLBACK;
