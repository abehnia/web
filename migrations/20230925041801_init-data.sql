CREATE TABLE IF NOT EXISTS report (
    id            TEXT    PRIMARY KEY NOT NULL,
    gross_revenue CHARACTER(50)       NOT NULL,
    expenses      CHARACTER(50)       NOT NULL
);

CREATE TABLE IF NOT EXISTS transactions (
    id            TEXT    PRIMARY KEY NOT NULL,
    date          DATETIME            NOT NULL,
    amount        CHARACTER(50)       NOT NULL,
    memo          VARCHAR(100)        NOT NULL
);
