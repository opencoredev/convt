-- Reverses 0000_init. Local development only: production is forward-only and
-- recovers from backups. `bun run db:rollback` runs this and deletes the
-- migration's row from drizzle.__drizzle_migrations in the same transaction.
drop table usage_events, cloud_jobs, api_keys, invoices, licenses, subscriptions, orders,
  devices, otp_send_limits, rate_limits, verifications, accounts, sessions, users;
drop function usage_events_guard_update();
drop function usage_events_guard_delete();
revoke usage on schema public from convt_web, convt_server;
