-- Reference copy of the schema the app creates and migrates automatically
-- on first connection to a SQL Server database. This file is not read by
-- the app and is not authoritative -- it exists so a DBA can see what a
-- login needs before ever running the app. The real, always-current version
-- is the statements in src/db/mssql.rs (see `migrate()`).

IF NOT EXISTS (SELECT 1 FROM sys.tables WHERE name = 'categories')
CREATE TABLE categories (
    id    INT IDENTITY PRIMARY KEY,
    owner NVARCHAR(255) NOT NULL,
    name  NVARCHAR(64) COLLATE Latin1_General_CI_AS NOT NULL,
    CONSTRAINT categories_owner_name UNIQUE (owner, name)
);

IF NOT EXISTS (SELECT 1 FROM sys.tables WHERE name = 'spending')
CREATE TABLE spending (
    id           INT IDENTITY PRIMARY KEY,
    owner        NVARCHAR(255) NOT NULL,
    category_id  INT NOT NULL REFERENCES categories(id),
    spent_on     DATE NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency     CHAR(3) NOT NULL,
    description  NVARCHAR(255) NOT NULL DEFAULT ''
);

IF NOT EXISTS (
    SELECT 1 FROM sys.indexes
     WHERE name = 'spending_by_date' AND object_id = OBJECT_ID('dbo.spending')
)
CREATE INDEX spending_by_date ON spending(spent_on);

-- `owner` holds the login that wrote each row (see "Where the data is kept"
-- in the README) and is filtered on for every read; it is not shown in the
-- app itself.

-- A database created before per-owner data existed gets upgraded to the
-- above automatically, the first time any login connects to it, rather than
-- running the CREATE TABLEs (which only apply IF NOT EXISTS):
--
--   ALTER TABLE categories ADD owner NVARCHAR(255) NOT NULL DEFAULT '';
--   ALTER TABLE spending   ADD owner NVARCHAR(255) NOT NULL DEFAULT '';
--   UPDATE categories SET owner = @connecting_login WHERE owner = '';
--   UPDATE spending   SET owner = @connecting_login WHERE owner = '';
--   -- then the old database-wide UNIQUE(name) constraint is dropped -- its
--   -- auto-generated name is looked up via sys.key_constraints, not
--   -- guessed -- and replaced with:
--   ALTER TABLE categories ADD CONSTRAINT categories_owner_name UNIQUE (owner, name);

-- Minimum grants for a login the app will use, run by an admin connected to
-- the target database. This is the exact sequence this project needed to
-- get a fresh SQL Server install working end to end:
--
--   USE the_database;
--   ALTER ROLE db_datareader ADD MEMBER someone;  -- SELECT
--   ALTER ROLE db_datawriter ADD MEMBER someone;  -- INSERT/UPDATE/DELETE
--   GRANT REFERENCES ON dbo.categories TO someone;
--
-- SQL Server splits CREATE TABLE-style DDL rights out from data access more
-- finely than MariaDB does, and -- unlike MariaDB -- REFERENCES has to be
-- granted on a specific table rather than covered by a blanket privilege.
-- If this database will only ever be this app's, simplest is to skip the
-- above and just make the login db_owner instead:
--
--   ALTER ROLE db_owner ADD MEMBER someone;
