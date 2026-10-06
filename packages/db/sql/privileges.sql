-- Grants and triggers for the current schema. Migration 0000 ends with this text;
-- later migrations add their own changes here too, and `db:ci` checks that the
-- migrations and (Drizzle schema + this file) build the same database.
--
-- convt_web is the Worker (through Hyperdrive); convt_server is convt-server and
-- convt-worker. Neither may delete financial rows or rewrite usage facts.
grant usage on schema public to convt_web, convt_server;
--> statement-breakpoint
grant select, insert, update on users to convt_web;
--> statement-breakpoint
grant select, insert, update, delete on sessions, accounts, verifications, rate_limits to convt_web;
--> statement-breakpoint
grant select, insert, update on otp_send_limits to convt_web;
--> statement-breakpoint
-- insert since 0002_device_auth: desktop sign-in (P8) creates device rows.
grant select, insert, update on devices to convt_web;
--> statement-breakpoint
grant select on orders, subscriptions, licenses, invoices to convt_web;
--> statement-breakpoint
grant select on api_keys, cloud_jobs, usage_events to convt_web;
--> statement-breakpoint
grant select on users, subscriptions, licenses to convt_server;
--> statement-breakpoint
grant select, delete on verifications, rate_limits, otp_send_limits to convt_server;
--> statement-breakpoint
grant select, insert, update on devices, cloud_jobs to convt_server;
--> statement-breakpoint
grant select on api_keys to convt_server;
--> statement-breakpoint
grant update (last_used_at) on api_keys to convt_server;
--> statement-breakpoint
grant select, insert on usage_events to convt_server;
--> statement-breakpoint
grant update (reported_at, provider_event_id) on usage_events to convt_server;
--> statement-breakpoint
-- Usage facts are append-only. An update may only set reported_at and
-- provider_event_id once (from null), or null out user_id, subscription_id or
-- api_key_id, which is how ON DELETE SET NULL anonymizes rows. Corrections are new
-- rows with kind 'correction'.
create function usage_events_guard_update() returns trigger
language plpgsql as $$
begin
  if new.id is distinct from old.id
    or new.job_id is distinct from old.job_id
    or new.kind is distinct from old.kind
    or new.quantity is distinct from old.quantity
    or new.amount_cents is distinct from old.amount_cents
    or new.corrects is distinct from old.corrects
    or new.occurred_at is distinct from old.occurred_at
    or new.created_at is distinct from old.created_at
    or (new.user_id is distinct from old.user_id and new.user_id is not null)
    or (new.subscription_id is distinct from old.subscription_id and new.subscription_id is not null)
    or (new.api_key_id is distinct from old.api_key_id and new.api_key_id is not null)
    or (new.reported_at is distinct from old.reported_at and old.reported_at is not null)
    or (new.provider_event_id is distinct from old.provider_event_id and old.provider_event_id is not null)
  then
    raise exception 'usage_events rows are append-only'
      using errcode = 'insufficient_privilege';
  end if;
  return new;
end
$$;
--> statement-breakpoint
create function usage_events_guard_delete() returns trigger
language plpgsql as $$
begin
  raise exception 'usage_events rows are never deleted'
    using errcode = 'insufficient_privilege';
end
$$;
--> statement-breakpoint
create trigger usage_events_no_rewrite before update on usage_events
  for each row execute function usage_events_guard_update();
--> statement-breakpoint
create trigger usage_events_no_delete before delete on usage_events
  for each row execute function usage_events_guard_delete();
--> statement-breakpoint
-- Billing (migration 0001). convt_billing is the convt-billing Worker: it alone
-- writes purchases, licenses and the tables around them. convt_web reads them and
-- claims purchases only through claim_purchases.
grant usage on schema public to convt_billing;
--> statement-breakpoint
grant select on checkouts, billing_customers, disputes, account_deletions to convt_web;
--> statement-breakpoint
grant select, insert, update on orders, subscriptions, licenses, invoices, payment_coverage,
  disputes, checkouts, billing_customers, webhook_events, email_outbox, account_deletions,
  reconcile_runs, reconcile_cursors, billing_alerts to convt_billing;
--> statement-breakpoint
-- Only the retention jobs delete, and only these two tables.
grant delete on email_outbox, webhook_events to convt_billing;
--> statement-breakpoint
grant select on users, usage_events, cloud_jobs to convt_billing;
--> statement-breakpoint
-- Attaches purchases made before the account existed: locks the user, requires a
-- verified email, and attaches unclaimed rows with exactly that email. Called for
-- any user id, it can only give that user their own email's purchases.
create function claim_purchases(p_user_id text)
returns table (claimed_orders integer, claimed_subscriptions integer, claimed_licenses integer, claimed_invoices integer)
language plpgsql security definer set search_path = public, pg_temp as $$
declare
  v_email text;
  v_verified boolean;
  n_orders integer := 0;
  n_subscriptions integer := 0;
  n_licenses integer := 0;
  n_invoices integer := 0;
begin
  -- Ingest takes the same lock before it writes a user's rows, so the two never
  -- wait on each other's row locks in opposite orders.
  perform pg_advisory_xact_lock(hashtextextended('user:' || p_user_id, 0));
  select u.email, u.email_verified into v_email, v_verified from users u where u.id = p_user_id for update;
  if not found or not v_verified then
    return query select 0, 0, 0, 0;
    return;
  end if;
  update orders o set user_id = p_user_id, updated_at = now() where o.user_id is null and o.email = v_email;
  get diagnostics n_orders = row_count;
  update subscriptions s set user_id = p_user_id, updated_at = now() where s.user_id is null and s.email = v_email;
  get diagnostics n_subscriptions = row_count;
  update licenses l set user_id = p_user_id, updated_at = now() where l.user_id is null and l.email = v_email;
  get diagnostics n_licenses = row_count;
  update invoices i set user_id = p_user_id, updated_at = now() where i.user_id is null and i.email = v_email;
  get diagnostics n_invoices = row_count;
  return query select n_orders, n_subscriptions, n_licenses, n_invoices;
end
$$;
--> statement-breakpoint
revoke all on function claim_purchases(text) from public;
--> statement-breakpoint
grant execute on function claim_purchases(text) to convt_web, convt_billing;
--> statement-breakpoint
-- The last step of an account deletion. Refuses unless the deletion row is in
-- `deleting` and no subscription of the user is still live. Financial rows keep
-- their email; foreign keys set their user_id to null.
create function delete_user(p_user_id text, p_deletion_id text)
returns boolean
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  perform pg_advisory_xact_lock(hashtextextended('user:' || p_user_id, 0));
  perform 1 from account_deletions d
    where d.id = p_deletion_id and d.user_id = p_user_id and d.status = 'deleting' for update;
  if not found then
    raise exception 'delete_user: deletion % is not in deleting for this user', p_deletion_id
      using errcode = 'P0001';
  end if;
  if exists (
    select 1 from subscriptions s
    where s.user_id = p_user_id
      and s.status not in ('canceled', 'incomplete_expired')
      and (s.ended_at is null or s.ended_at > now())
  ) then
    raise exception 'delete_user: user % still has a live subscription', p_user_id
      using errcode = 'P0001';
  end if;
  update email_outbox set status = 'skipped', finished_at = now(), updated_at = now()
    where user_id = p_user_id and status = 'pending' and kind <> 'license_issued';
  delete from users where id = p_user_id;
  update account_deletions set status = 'done', finished_at = now(), updated_at = now()
    where id = p_deletion_id;
  return true;
end
$$;
--> statement-breakpoint
revoke all on function delete_user(text, text) from public;
--> statement-breakpoint
grant execute on function delete_user(text, text) to convt_billing;
--> statement-breakpoint
-- An issued license is a fact: its signed fields never change, and a revocation is
-- never undone. A claim may set user_id from null and a deletion may null it. The
-- owner (migrations, the local seed) is exempt.
create function licenses_guard_update() returns trigger
language plpgsql as $$
begin
  if current_user = 'convt_owner' then
    return new;
  end if;
  if new.id is distinct from old.id
    or new.email is distinct from old.email
    or new.plan is distinct from old.plan
    or new.trial is distinct from old.trial
    or new.order_id is distinct from old.order_id
    or new.subscription_id is distinct from old.subscription_id
    or new.invoice_id is distinct from old.invoice_id
    or new.period_start is distinct from old.period_start
    or new.issued_on is distinct from old.issued_on
    or new.updates_until is distinct from old.updates_until
    or new.token is distinct from old.token
    or new.reissue_of is distinct from old.reissue_of
    or new.created_at is distinct from old.created_at
    or (old.revoked_at is not null
      and (new.revoked_at is distinct from old.revoked_at or new.revoke_reason is distinct from old.revoke_reason))
    or (new.user_id is distinct from old.user_id and old.user_id is not null and new.user_id is not null)
  then
    raise exception 'licenses rows do not change once issued'
      using errcode = 'insufficient_privilege';
  end if;
  return new;
end
$$;
--> statement-breakpoint
create trigger licenses_no_rewrite before update on licenses
  for each row execute function licenses_guard_update();
--> statement-breakpoint
-- P9: web keys, per-key rate buckets, and subscription row locking for reservations.
grant insert, update on api_keys to convt_web;
--> statement-breakpoint
grant insert, update on otp_send_limits to convt_server;
--> statement-breakpoint
grant update (updated_at) on subscriptions to convt_server, convt_web;
