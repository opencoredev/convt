-- Roles for the convt database. Run once per cluster as a superuser, before the
-- first migration:
--
--   psql "$SUPERUSER_URL" -v owner_password=... -v web_password=... \
--     -v server_password=... -v billing_password=... -f packages/db/roles.sql
--
-- scripts/db.sh runs it for local containers. On Railway, Leo runs it once with
-- passwords from a password manager. Running it again updates the passwords.
--
-- convt_owner owns every table and runs migrations. convt_web (the site Worker
-- through Hyperdrive), convt_billing (the convt-billing Worker through its own
-- Hyperdrive config) and convt_server (convt-server and convt-worker) get only the
-- grants that the migrations give them.

\set ON_ERROR_STOP on

select format('create role %I login', r)
from unnest(array['convt_owner', 'convt_web', 'convt_server', 'convt_billing']) as r
where not exists (select 1 from pg_roles where rolname = r)
\gexec

alter role convt_owner with login nosuperuser nocreatedb nocreaterole password :'owner_password';
alter role convt_web with login nosuperuser nocreatedb nocreaterole password :'web_password';
alter role convt_server with login nosuperuser nocreatedb nocreaterole password :'server_password';
alter role convt_billing with login nosuperuser nocreatedb nocreaterole password :'billing_password';

-- The owner of the database owns its public schema (PostgreSQL 15 and later), so
-- migrations can create tables there and grant on them.
select format('alter database %I owner to convt_owner', current_database())
\gexec

revoke create on schema public from public;

-- Drizzle's migrator keeps its bookkeeping in the drizzle schema. convt-server reads
-- it at startup to check that the database has the migrations it was built for.
create schema if not exists drizzle authorization convt_owner;
grant usage on schema drizzle to convt_server;
alter default privileges for role convt_owner in schema drizzle grant select on tables to convt_server;
grant select on all tables in schema drizzle to convt_server;
grant connect on database :"DBNAME" to convt_web, convt_server, convt_billing;
