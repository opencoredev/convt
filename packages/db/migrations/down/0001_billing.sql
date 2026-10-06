-- Reverses 0001_billing. Local development only: production is forward-only. The
-- restored P6 checks are NOT VALID so rows written since 0001 (pending orders, new
-- statuses) do not block a rollback.
drop trigger licenses_no_rewrite on licenses;
drop function licenses_guard_update();
drop function delete_user(text, text);
drop function claim_purchases(text);
revoke all on all tables in schema public from convt_billing;
revoke usage on schema public from convt_billing;
grant update on orders, subscriptions, licenses, invoices to convt_web;

alter table licenses drop constraint licenses_revoke_check;
drop index licenses_pro_invoice_key;
drop index licenses_pro_period_until_key;
alter table licenses drop column invoice_id;
create unique index licenses_pro_period_key on licenses using btree (subscription_id, period_start)
  where plan = 'pro' and reissue_of is null;

alter table orders drop constraint orders_paid_at_check;
alter table orders drop constraint orders_refunded_cents_check;
alter table orders drop constraint orders_status_check;
alter table orders drop column billed_at, drop column refunded_cents, drop column checkout_id,
  drop column provider_version, drop column provider_hash;
update orders set paid_at = coalesce(paid_at, created_at);
alter table orders alter column paid_at set not null;
alter table orders add constraint orders_status_check
  check (status in ('paid', 'refunded', 'partially_refunded')) not valid;

alter table subscriptions drop constraint subscriptions_spend_cap_check;
alter table subscriptions drop constraint subscriptions_status_check;
drop index subscriptions_status_idx;
alter table subscriptions drop column checkout_id, drop column provider_version,
  drop column provider_hash, drop column pending_update, drop column card_seen_at;
alter table subscriptions add column provider_updated_at timestamp with time zone;
alter table subscriptions add constraint subscriptions_status_check
  check (status in ('trialing', 'active', 'past_due', 'canceled', 'unpaid', 'incomplete')) not valid;

alter table invoices drop constraint invoices_paid_at_check;
alter table invoices drop constraint invoices_refunded_cents_check;
alter table invoices drop constraint invoices_status_check;
drop index invoices_subscription_idx;
alter table invoices drop column reason, drop column billed_at, drop column paid_at,
  drop column net_cents, drop column applied_balance_cents, drop column refunded_cents,
  drop column provider_version, drop column provider_hash;
alter table invoices add constraint invoices_status_check
  check (status in ('draft', 'open', 'paid', 'void', 'uncollectible', 'refunded')) not valid;

drop table payment_coverage, disputes, billing_alerts, reconcile_cursors, reconcile_runs,
  account_deletions, email_outbox, webhook_events, billing_customers, checkouts;
