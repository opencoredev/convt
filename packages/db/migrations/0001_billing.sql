CREATE TABLE "account_deletions" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text,
	"status" text DEFAULT 'pending' NOT NULL,
	"attempts" integer DEFAULT 0 NOT NULL,
	"next_attempt_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_error" text,
	"alerted_at" timestamp with time zone,
	"finished_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "account_deletions_status_check" CHECK ("account_deletions"."status" in ('pending', 'canceling', 'deleting', 'done', 'failed'))
);
--> statement-breakpoint
CREATE TABLE "billing_alerts" (
	"id" text PRIMARY KEY NOT NULL,
	"kind" text NOT NULL,
	"subject" text NOT NULL,
	"detail" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"digested_at" timestamp with time zone
);
--> statement-breakpoint
CREATE TABLE "billing_customers" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_customer_id" text NOT NULL,
	"email" text NOT NULL,
	"deleted_at" timestamp with time zone,
	"provider_version" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "checkouts" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_checkout_id" text,
	"user_id" text,
	"product" text NOT NULL,
	"allow_trial" boolean DEFAULT false NOT NULL,
	"spend_cap_cents" integer,
	"nonce_hash" "bytea" NOT NULL,
	"nonce_expires_at" timestamp with time zone NOT NULL,
	"key_disclosed_at" timestamp with time zone,
	"status" text DEFAULT 'created' NOT NULL,
	"synced_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "checkouts_product_check" CHECK ("checkouts"."product" in ('desktop', 'pro_month', 'pro_year', 'api')),
	CONSTRAINT "checkouts_status_check" CHECK ("checkouts"."status" in ('created', 'open', 'expired', 'confirmed', 'succeeded', 'failed')),
	CONSTRAINT "checkouts_spend_cap_check" CHECK (("checkouts"."product" = 'api' and "checkouts"."spend_cap_cents" is not null and "checkouts"."spend_cap_cents" > 0) or ("checkouts"."product" <> 'api' and "checkouts"."spend_cap_cents" is null))
);
--> statement-breakpoint
CREATE TABLE "disputes" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_dispute_id" text NOT NULL,
	"order_id" text,
	"invoice_id" text,
	"status" text NOT NULL,
	"amount_cents" integer NOT NULL,
	"closed" boolean DEFAULT false NOT NULL,
	"provider_version" timestamp with time zone,
	"provider_hash" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "disputes_status_check" CHECK ("disputes"."status" in ('prevented', 'early_warning', 'needs_response', 'under_review', 'lost', 'won')),
	CONSTRAINT "disputes_subject_check" CHECK (num_nonnulls("disputes"."order_id", "disputes"."invoice_id") = 1)
);
--> statement-breakpoint
CREATE TABLE "email_outbox" (
	"id" text PRIMARY KEY NOT NULL,
	"kind" text NOT NULL,
	"dedupe_key" text NOT NULL,
	"to_email" text NOT NULL,
	"user_id" text,
	"subject_id" text NOT NULL,
	"status" text DEFAULT 'pending' NOT NULL,
	"claim_generation" integer DEFAULT 0 NOT NULL,
	"locked_until" timestamp with time zone,
	"next_attempt_at" timestamp with time zone DEFAULT now() NOT NULL,
	"attempts" integer DEFAULT 0 NOT NULL,
	"first_attempt_at" timestamp with time zone,
	"last_attempt_at" timestamp with time zone,
	"unknown_outcome_at" timestamp with time zone,
	"template_version" integer,
	"payload" jsonb,
	"payload_sha256" text,
	"provider_message_id" text,
	"last_error" text,
	"sent_at" timestamp with time zone,
	"finished_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "email_outbox_kind_check" CHECK ("email_outbox"."kind" in ('license_issued', 'trial_ending', 'renewal_failed', 'alert_digest')),
	CONSTRAINT "email_outbox_status_check" CHECK ("email_outbox"."status" in ('pending', 'sending', 'sent', 'skipped', 'dead', 'ambiguous'))
);
--> statement-breakpoint
CREATE TABLE "payment_coverage" (
	"id" text PRIMARY KEY NOT NULL,
	"invoice_id" text NOT NULL,
	"provider_item_id" text NOT NULL,
	"subscription_id" text NOT NULL,
	"product" text NOT NULL,
	"price_id" text NOT NULL,
	"period_start" timestamp with time zone NOT NULL,
	"period_end" timestamp with time zone NOT NULL,
	"amount_cents" integer NOT NULL,
	"kind" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "payment_coverage_kind_check" CHECK ("payment_coverage"."kind" in ('period', 'proration')),
	CONSTRAINT "payment_coverage_product_check" CHECK ("payment_coverage"."product" in ('pro_month', 'pro_year')),
	CONSTRAINT "payment_coverage_period_check" CHECK ("payment_coverage"."period_end" > "payment_coverage"."period_start")
);
--> statement-breakpoint
CREATE TABLE "reconcile_cursors" (
	"name" text PRIMARY KEY NOT NULL,
	"page" bigint DEFAULT 1 NOT NULL,
	"pass_started_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "reconcile_runs" (
	"id" text PRIMARY KEY NOT NULL,
	"kind" text NOT NULL,
	"started_at" timestamp with time zone NOT NULL,
	"finished_at" timestamp with time zone,
	"status" text DEFAULT 'running' NOT NULL,
	"summary" jsonb DEFAULT '{}'::jsonb NOT NULL,
	CONSTRAINT "reconcile_runs_kind_check" CHECK ("reconcile_runs"."kind" in ('frequent', 'daily')),
	CONSTRAINT "reconcile_runs_status_check" CHECK ("reconcile_runs"."status" in ('running', 'ok', 'failed'))
);
--> statement-breakpoint
CREATE TABLE "webhook_events" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_event_id" text NOT NULL,
	"type" text NOT NULL,
	"received_at" timestamp with time zone DEFAULT now() NOT NULL,
	"body" text,
	"status" text NOT NULL,
	"reason" text,
	"attempts" integer DEFAULT 0 NOT NULL,
	"processed_at" timestamp with time zone,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "webhook_events_status_check" CHECK ("webhook_events"."status" in ('processed', 'ignored', 'rejected', 'failed', 'dead'))
);
--> statement-breakpoint
ALTER TABLE "invoices" DROP CONSTRAINT "invoices_status_check";--> statement-breakpoint
ALTER TABLE "orders" DROP CONSTRAINT "orders_status_check";--> statement-breakpoint
ALTER TABLE "subscriptions" DROP CONSTRAINT "subscriptions_status_check";--> statement-breakpoint
DROP INDEX "licenses_pro_period_key";--> statement-breakpoint
ALTER TABLE "orders" ALTER COLUMN "paid_at" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "reason" text;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "billed_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "paid_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "net_cents" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "applied_balance_cents" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "refunded_cents" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "provider_version" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "invoices" ADD COLUMN "provider_hash" text;--> statement-breakpoint
ALTER TABLE "licenses" ADD COLUMN "invoice_id" text;--> statement-breakpoint
ALTER TABLE "orders" ADD COLUMN "billed_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "orders" ADD COLUMN "refunded_cents" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "orders" ADD COLUMN "checkout_id" text;--> statement-breakpoint
ALTER TABLE "orders" ADD COLUMN "provider_version" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "orders" ADD COLUMN "provider_hash" text;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD COLUMN "checkout_id" text;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD COLUMN "provider_version" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD COLUMN "provider_hash" text;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD COLUMN "pending_update" jsonb;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD COLUMN "card_seen_at" timestamp with time zone;--> statement-breakpoint
-- P6 rows: every order was paid, and invoices were issued when billed.
UPDATE "orders" SET "billed_at" = "paid_at";--> statement-breakpoint
UPDATE "invoices" SET "billed_at" = "issued_at", "net_cents" = "amount_cents",
  "paid_at" = CASE WHEN "status" IN ('paid', 'partially_refunded', 'refunded') THEN "issued_at" END;--> statement-breakpoint
ALTER TABLE "orders" ALTER COLUMN "billed_at" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "invoices" ALTER COLUMN "billed_at" SET NOT NULL;--> statement-breakpoint
-- The old grants let convt_web update purchases; from here only convt_billing writes them.
REVOKE UPDATE ON "orders", "subscriptions", "licenses", "invoices" FROM convt_web;--> statement-breakpoint
ALTER TABLE "account_deletions" ADD CONSTRAINT "account_deletions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "billing_customers" ADD CONSTRAINT "billing_customers_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "checkouts" ADD CONSTRAINT "checkouts_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "disputes" ADD CONSTRAINT "disputes_order_id_orders_id_fk" FOREIGN KEY ("order_id") REFERENCES "public"."orders"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "disputes" ADD CONSTRAINT "disputes_invoice_id_invoices_id_fk" FOREIGN KEY ("invoice_id") REFERENCES "public"."invoices"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "email_outbox" ADD CONSTRAINT "email_outbox_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "payment_coverage" ADD CONSTRAINT "payment_coverage_invoice_id_invoices_id_fk" FOREIGN KEY ("invoice_id") REFERENCES "public"."invoices"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "payment_coverage" ADD CONSTRAINT "payment_coverage_subscription_id_subscriptions_id_fk" FOREIGN KEY ("subscription_id") REFERENCES "public"."subscriptions"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "account_deletions_open_key" ON "account_deletions" USING btree ("user_id") WHERE "account_deletions"."status" <> 'done';--> statement-breakpoint
CREATE INDEX "account_deletions_due_idx" ON "account_deletions" USING btree ("next_attempt_at") WHERE "account_deletions"."status" not in ('done');--> statement-breakpoint
CREATE UNIQUE INDEX "billing_alerts_kind_subject_key" ON "billing_alerts" USING btree ("kind","subject");--> statement-breakpoint
CREATE INDEX "billing_alerts_undigested_idx" ON "billing_alerts" USING btree ("created_at") WHERE "billing_alerts"."digested_at" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "billing_customers_provider_customer_key" ON "billing_customers" USING btree ("provider","provider_customer_id");--> statement-breakpoint
CREATE UNIQUE INDEX "billing_customers_provider_user_key" ON "billing_customers" USING btree ("provider","user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "checkouts_provider_checkout_key" ON "checkouts" USING btree ("provider","provider_checkout_id");--> statement-breakpoint
CREATE INDEX "checkouts_user_idx" ON "checkouts" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "checkouts_open_idx" ON "checkouts" USING btree ("created_at") WHERE "checkouts"."status" in ('created', 'open');--> statement-breakpoint
CREATE UNIQUE INDEX "disputes_provider_dispute_key" ON "disputes" USING btree ("provider","provider_dispute_id");--> statement-breakpoint
CREATE INDEX "disputes_order_idx" ON "disputes" USING btree ("order_id");--> statement-breakpoint
CREATE INDEX "disputes_invoice_idx" ON "disputes" USING btree ("invoice_id");--> statement-breakpoint
CREATE INDEX "disputes_open_idx" ON "disputes" USING btree ("updated_at") WHERE "disputes"."status" not in ('lost', 'won');--> statement-breakpoint
CREATE UNIQUE INDEX "email_outbox_dedupe_key" ON "email_outbox" USING btree ("dedupe_key");--> statement-breakpoint
CREATE INDEX "email_outbox_due_idx" ON "email_outbox" USING btree ("next_attempt_at") WHERE "email_outbox"."status" in ('pending', 'sending');--> statement-breakpoint
CREATE INDEX "email_outbox_user_idx" ON "email_outbox" USING btree ("user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "payment_coverage_item_key" ON "payment_coverage" USING btree ("provider_item_id");--> statement-breakpoint
CREATE INDEX "payment_coverage_subscription_idx" ON "payment_coverage" USING btree ("subscription_id");--> statement-breakpoint
CREATE INDEX "payment_coverage_invoice_idx" ON "payment_coverage" USING btree ("invoice_id");--> statement-breakpoint
CREATE INDEX "reconcile_runs_kind_started_idx" ON "reconcile_runs" USING btree ("kind","started_at" DESC NULLS LAST);--> statement-breakpoint
CREATE UNIQUE INDEX "webhook_events_provider_event_key" ON "webhook_events" USING btree ("provider","provider_event_id");--> statement-breakpoint
CREATE INDEX "webhook_events_status_idx" ON "webhook_events" USING btree ("status") WHERE "webhook_events"."status" in ('failed', 'rejected', 'dead');--> statement-breakpoint
CREATE INDEX "webhook_events_received_idx" ON "webhook_events" USING btree ("received_at");--> statement-breakpoint
ALTER TABLE "licenses" ADD CONSTRAINT "licenses_invoice_id_invoices_id_fk" FOREIGN KEY ("invoice_id") REFERENCES "public"."invoices"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_checkout_id_checkouts_id_fk" FOREIGN KEY ("checkout_id") REFERENCES "public"."checkouts"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD CONSTRAINT "subscriptions_checkout_id_checkouts_id_fk" FOREIGN KEY ("checkout_id") REFERENCES "public"."checkouts"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "invoices_subscription_idx" ON "invoices" USING btree ("subscription_id");--> statement-breakpoint
CREATE UNIQUE INDEX "licenses_pro_period_until_key" ON "licenses" USING btree ("subscription_id","period_start","updates_until") WHERE "licenses"."plan" = 'pro' and "licenses"."reissue_of" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "licenses_pro_invoice_key" ON "licenses" USING btree ("invoice_id") WHERE "licenses"."plan" = 'pro' and "licenses"."reissue_of" is null;--> statement-breakpoint
CREATE INDEX "subscriptions_status_idx" ON "subscriptions" USING btree ("status");--> statement-breakpoint
ALTER TABLE "subscriptions" DROP COLUMN "provider_updated_at";--> statement-breakpoint
ALTER TABLE "invoices" ADD CONSTRAINT "invoices_paid_at_check" CHECK ("invoices"."status" not in ('paid', 'partially_refunded', 'refunded') or "invoices"."paid_at" is not null);--> statement-breakpoint
ALTER TABLE "invoices" ADD CONSTRAINT "invoices_refunded_cents_check" CHECK ("invoices"."refunded_cents" >= 0);--> statement-breakpoint
ALTER TABLE "invoices" ADD CONSTRAINT "invoices_status_check" CHECK ("invoices"."status" in ('draft', 'open', 'paid', 'partially_refunded', 'void', 'uncollectible', 'refunded'));--> statement-breakpoint
ALTER TABLE "licenses" ADD CONSTRAINT "licenses_revoke_check" CHECK (("licenses"."revoked_at" is null and "licenses"."revoke_reason" is null) or ("licenses"."revoked_at" is not null and "licenses"."revoke_reason" in ('refunded', 'dispute_lost')));--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_paid_at_check" CHECK ("orders"."status" not in ('paid', 'partially_refunded', 'refunded') or "orders"."paid_at" is not null);--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_refunded_cents_check" CHECK ("orders"."refunded_cents" >= 0);--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_status_check" CHECK ("orders"."status" in ('pending', 'paid', 'partially_refunded', 'refunded', 'void'));--> statement-breakpoint
ALTER TABLE "subscriptions" ADD CONSTRAINT "subscriptions_spend_cap_check" CHECK (("subscriptions"."kind" = 'pro' and "subscriptions"."spend_cap_cents" is null) or ("subscriptions"."kind" = 'api' and "subscriptions"."spend_cap_cents" is not null and "subscriptions"."spend_cap_cents" > 0));--> statement-breakpoint
ALTER TABLE "subscriptions" ADD CONSTRAINT "subscriptions_status_check" CHECK ("subscriptions"."status" in ('incomplete', 'incomplete_expired', 'trialing', 'active', 'past_due', 'canceled', 'unpaid', 'paused'));--> statement-breakpoint
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
