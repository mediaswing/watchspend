-- Reference copy of the schema the app creates and migrates automatically
-- on first connection to a MariaDB/MySQL database. This file is not read by
-- the app and is not authoritative -- it exists so a DBA can see what a
-- login needs before ever running the app. The real, always-current version
-- is the CREATE TABLE statements in src/db/mariadb.rs (see `migrate()`).

CREATE TABLE IF NOT EXISTS categories (
    id    INT AUTO_INCREMENT PRIMARY KEY,
    owner VARCHAR(255) NOT NULL,
    name  VARCHAR(64) COLLATE utf8mb4_unicode_ci NOT NULL,
    CONSTRAINT categories_owner_name UNIQUE (owner, name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS spending (
    id           INT AUTO_INCREMENT PRIMARY KEY,
    owner        VARCHAR(255) NOT NULL,
    category_id  INT NOT NULL,
    spent_on     DATE NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency     CHAR(3) NOT NULL,
    description  VARCHAR(255) NOT NULL DEFAULT '',
    INDEX spending_by_date (spent_on),
    FOREIGN KEY (category_id) REFERENCES categories(id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- `owner` holds the login that wrote each row (see "Where the data is kept"
-- in the README) and is filtered on for every read; it is not shown in the
-- app itself.

-- A database created before per-owner data existed gets upgraded to the
-- above automatically, the first time any login connects to it, rather than
-- running the CREATE TABLEs (which only apply IF NOT EXISTS):
--
--   ALTER TABLE categories ADD COLUMN owner VARCHAR(255) NOT NULL DEFAULT '';
--   ALTER TABLE spending   ADD COLUMN owner VARCHAR(255) NOT NULL DEFAULT '';
--   UPDATE categories SET owner = '<connecting login>' WHERE owner = '';
--   UPDATE spending   SET owner = '<connecting login>' WHERE owner = '';
--   -- then the old database-wide UNIQUE(name) is dropped -- its
--   -- auto-generated name is looked up, not guessed -- and replaced with:
--   ALTER TABLE categories ADD CONSTRAINT categories_owner_name UNIQUE (owner, name);

-- Minimum grant for a login the app will use, run by an admin connected to
-- the target database:
--
--   GRANT CREATE, ALTER, SELECT, INSERT, REFERENCES
--     ON accounts.* TO 'someone'@'%' IDENTIFIED BY 'a password';
--
-- REFERENCES is needed because MariaDB/MySQL require it on the parent table
-- to create the foreign key from spending to categories -- it is not
-- covered by CREATE. ALTER is only needed the moment an existing database
-- is upgraded to the per-owner schema above; it is not used afterwards.
