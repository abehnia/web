CREATE TABLE IF NOT EXISTS report (
    id            INTEGER PRIMARY KEY NOT NULL CHECK(id = 0),
    gross_revenue CHARACTER(50)       NOT NULL,
    expenses      CHARACTER(50)       NOT NULL,
    net_revenue   CHARACTER(50)       NOT NULL
);

CREATE TABLE IF NOT EXISTS transactions (
    id            TEXT    PRIMARY KEY NOT NULL,
    date          DATETIME            NOT NULL,
    income        BOOLEAN             NOT NULL,
    amount        CHARACTER(50)       NOT NULL,
    memo          VARCHAR(100)        NOT NULL
);

INSERT INTO report (id, gross_revenue, expenses, net_revenue)
VALUES
(0, '0', '0', '0')

