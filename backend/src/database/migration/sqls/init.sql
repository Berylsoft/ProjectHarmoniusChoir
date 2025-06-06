CREATE TABLE IF NOT EXISTS "__migrations" (
    "id" INTEGER NOT NULL UNIQUE,
    "version" INTEGER NOT NULL,
    PRIMARY KEY("id")
);
