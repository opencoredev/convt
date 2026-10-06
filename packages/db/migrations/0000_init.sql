CREATE TABLE "accounts" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"provider_id" text NOT NULL,
	"account_id" text NOT NULL,
	"access_token" text,
	"refresh_token" text,
	"id_token" text,
	"access_token_expires_at" timestamp with time zone,
	"refresh_token_expires_at" timestamp with time zone,
	"scope" text,
	"password" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "devices" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"name" text NOT NULL,
	"os" text NOT NULL,
	"app_version" text,
	"token_hash" text,
	"last_seen_at" timestamp with time zone,
	"revoked_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "otp_send_limits" (
	"key" text PRIMARY KEY NOT NULL,
	"window_start" timestamp with time zone NOT NULL,
	"count" integer NOT NULL,
	"expires_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "rate_limits" (
	"id" text PRIMARY KEY NOT NULL,
	"key" text NOT NULL,
	"count" integer NOT NULL,
	"last_request" bigint NOT NULL
);
--> statement-breakpoint
CREATE TABLE "sessions" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"token" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"ip_address" text,
	"user_agent" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "users" (
	"id" text PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"email" text NOT NULL,
	"email_verified" boolean DEFAULT false NOT NULL,
	"image" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "users_email_normalized" CHECK ("users"."email" = lower(btrim("users"."email")))
);
--> statement-breakpoint
CREATE TABLE "verifications" (
	"id" text PRIMARY KEY NOT NULL,
	"identifier" text NOT NULL,
	"value" text NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "invoices" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_invoice_id" text NOT NULL,
	"user_id" text,
	"email" text NOT NULL,
	"subscription_id" text,
	"order_id" text,
	"description" text NOT NULL,
	"amount_cents" integer NOT NULL,
	"currency" text NOT NULL,
	"status" text NOT NULL,
	"issued_at" timestamp with time zone NOT NULL,
	"receipt_url" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "invoices_status_check" CHECK ("invoices"."status" in ('draft', 'open', 'paid', 'void', 'uncollectible', 'refunded')),
	CONSTRAINT "invoices_email_normalized" CHECK ("invoices"."email" = lower(btrim("invoices"."email")))
);
--> statement-breakpoint
CREATE TABLE "licenses" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text,
	"email" text NOT NULL,
	"plan" text NOT NULL,
	"trial" boolean DEFAULT false NOT NULL,
	"order_id" text,
	"subscription_id" text,
	"period_start" date,
	"issued_on" date NOT NULL,
	"updates_until" date NOT NULL,
	"token" text NOT NULL,
	"reissue_of" text,
	"revoked_at" timestamp with time zone,
	"revoke_reason" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "licenses_plan_check" CHECK ("licenses"."plan" in ('desktop', 'pro')),
	CONSTRAINT "licenses_email_normalized" CHECK ("licenses"."email" = lower(btrim("licenses"."email"))),
	CONSTRAINT "licenses_source_check" CHECK (("licenses"."plan" = 'desktop' and "licenses"."order_id" is not null and "licenses"."subscription_id" is null) or ("licenses"."plan" = 'pro' and "licenses"."subscription_id" is not null and "licenses"."period_start" is not null))
);
--> statement-breakpoint
CREATE TABLE "orders" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_order_id" text NOT NULL,
	"provider_customer_id" text,
	"user_id" text,
	"email" text NOT NULL,
	"product" text NOT NULL,
	"amount_cents" integer NOT NULL,
	"currency" text NOT NULL,
	"status" text NOT NULL,
	"paid_at" timestamp with time zone NOT NULL,
	"refunded_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "orders_status_check" CHECK ("orders"."status" in ('paid', 'refunded', 'partially_refunded')),
	CONSTRAINT "orders_product_check" CHECK ("orders"."product" in ('desktop')),
	CONSTRAINT "orders_email_normalized" CHECK ("orders"."email" = lower(btrim("orders"."email")))
);
--> statement-breakpoint
CREATE TABLE "subscriptions" (
	"id" text PRIMARY KEY NOT NULL,
	"provider" text DEFAULT 'polar' NOT NULL,
	"provider_subscription_id" text NOT NULL,
	"provider_customer_id" text,
	"user_id" text,
	"email" text NOT NULL,
	"kind" text NOT NULL,
	"interval" text,
	"status" text NOT NULL,
	"trial_ends_at" timestamp with time zone,
	"current_period_start" timestamp with time zone,
	"current_period_end" timestamp with time zone,
	"cancel_at_period_end" boolean DEFAULT false NOT NULL,
	"canceled_at" timestamp with time zone,
	"ended_at" timestamp with time zone,
	"spend_cap_cents" integer,
	"provider_updated_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "subscriptions_kind_check" CHECK ("subscriptions"."kind" in ('pro', 'api')),
	CONSTRAINT "subscriptions_status_check" CHECK ("subscriptions"."status" in ('trialing', 'active', 'past_due', 'canceled', 'unpaid', 'incomplete')),
	CONSTRAINT "subscriptions_interval_check" CHECK (("subscriptions"."kind" = 'pro' and "subscriptions"."interval" is not null and "subscriptions"."interval" in ('month', 'year')) or ("subscriptions"."kind" = 'api' and "subscriptions"."interval" is null)),
	CONSTRAINT "subscriptions_email_normalized" CHECK ("subscriptions"."email" = lower(btrim("subscriptions"."email")))
);
--> statement-breakpoint
CREATE TABLE "api_keys" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"name" text NOT NULL,
	"prefix" text NOT NULL,
	"secret_hash" "bytea" NOT NULL,
	"last_used_at" timestamp with time zone,
	"revoked_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "cloud_jobs" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"source" text NOT NULL,
	"api_key_id" text,
	"status" text DEFAULT 'created' NOT NULL,
	"input_format" text NOT NULL,
	"target_format" text NOT NULL,
	"options" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"input_key" text,
	"input_bytes" bigint,
	"output_keys" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"attempt" integer DEFAULT 0 NOT NULL,
	"max_attempts" integer DEFAULT 3 NOT NULL,
	"lease_owner" text,
	"lease_expires_at" timestamp with time zone,
	"reserved_bytes" bigint DEFAULT 0 NOT NULL,
	"reserved_cents" integer DEFAULT 0 NOT NULL,
	"reservation" text DEFAULT 'open' NOT NULL,
	"error_code" text,
	"error_detail" text,
	"queued_at" timestamp with time zone,
	"started_at" timestamp with time zone,
	"finished_at" timestamp with time zone,
	"expires_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "cloud_jobs_source_check" CHECK ("cloud_jobs"."source" in ('api', 'web', 'desktop')),
	CONSTRAINT "cloud_jobs_status_check" CHECK ("cloud_jobs"."status" in ('created', 'uploaded', 'queued', 'running', 'succeeded', 'failed', 'cancelled')),
	CONSTRAINT "cloud_jobs_reservation_check" CHECK ("cloud_jobs"."reservation" in ('open', 'settled', 'released'))
);
--> statement-breakpoint
CREATE TABLE "usage_events" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text,
	"subscription_id" text,
	"api_key_id" text,
	"job_id" text,
	"kind" text NOT NULL,
	"quantity" bigint NOT NULL,
	"amount_cents" integer NOT NULL,
	"corrects" text,
	"occurred_at" timestamp with time zone NOT NULL,
	"reported_at" timestamp with time zone,
	"provider_event_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "usage_events_kind_check" CHECK ("usage_events"."kind" in ('api_conversion', 'pro_bytes', 'correction')),
	CONSTRAINT "usage_events_correction_check" CHECK (("usage_events"."kind" = 'correction') = ("usage_events"."corrects" is not null))
);
--> statement-breakpoint
ALTER TABLE "accounts" ADD CONSTRAINT "accounts_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "devices" ADD CONSTRAINT "devices_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invoices" ADD CONSTRAINT "invoices_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invoices" ADD CONSTRAINT "invoices_subscription_id_subscriptions_id_fk" FOREIGN KEY ("subscription_id") REFERENCES "public"."subscriptions"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invoices" ADD CONSTRAINT "invoices_order_id_orders_id_fk" FOREIGN KEY ("order_id") REFERENCES "public"."orders"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "licenses" ADD CONSTRAINT "licenses_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "licenses" ADD CONSTRAINT "licenses_order_id_orders_id_fk" FOREIGN KEY ("order_id") REFERENCES "public"."orders"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "licenses" ADD CONSTRAINT "licenses_subscription_id_subscriptions_id_fk" FOREIGN KEY ("subscription_id") REFERENCES "public"."subscriptions"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "licenses" ADD CONSTRAINT "licenses_reissue_of_licenses_id_fk" FOREIGN KEY ("reissue_of") REFERENCES "public"."licenses"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD CONSTRAINT "subscriptions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "api_keys" ADD CONSTRAINT "api_keys_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD CONSTRAINT "cloud_jobs_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD CONSTRAINT "cloud_jobs_api_key_id_api_keys_id_fk" FOREIGN KEY ("api_key_id") REFERENCES "public"."api_keys"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "usage_events" ADD CONSTRAINT "usage_events_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "usage_events" ADD CONSTRAINT "usage_events_subscription_id_subscriptions_id_fk" FOREIGN KEY ("subscription_id") REFERENCES "public"."subscriptions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "usage_events" ADD CONSTRAINT "usage_events_api_key_id_api_keys_id_fk" FOREIGN KEY ("api_key_id") REFERENCES "public"."api_keys"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "usage_events" ADD CONSTRAINT "usage_events_corrects_usage_events_id_fk" FOREIGN KEY ("corrects") REFERENCES "public"."usage_events"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "accounts_provider_account_key" ON "accounts" USING btree ("provider_id","account_id");--> statement-breakpoint
CREATE INDEX "accounts_user_id_idx" ON "accounts" USING btree ("user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "devices_token_hash_key" ON "devices" USING btree ("token_hash");--> statement-breakpoint
CREATE INDEX "devices_user_id_active_idx" ON "devices" USING btree ("user_id") WHERE "devices"."revoked_at" is null;--> statement-breakpoint
CREATE INDEX "otp_send_limits_expires_at_idx" ON "otp_send_limits" USING btree ("expires_at");--> statement-breakpoint
CREATE UNIQUE INDEX "rate_limits_key_key" ON "rate_limits" USING btree ("key");--> statement-breakpoint
CREATE UNIQUE INDEX "sessions_token_key" ON "sessions" USING btree ("token");--> statement-breakpoint
CREATE INDEX "sessions_user_id_idx" ON "sessions" USING btree ("user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "users_email_key" ON "users" USING btree ("email");--> statement-breakpoint
CREATE UNIQUE INDEX "verifications_identifier_key" ON "verifications" USING btree ("identifier");--> statement-breakpoint
CREATE INDEX "verifications_expires_at_idx" ON "verifications" USING btree ("expires_at");--> statement-breakpoint
CREATE UNIQUE INDEX "invoices_provider_invoice_key" ON "invoices" USING btree ("provider","provider_invoice_id");--> statement-breakpoint
CREATE INDEX "invoices_user_issued_idx" ON "invoices" USING btree ("user_id","issued_at" DESC NULLS LAST);--> statement-breakpoint
CREATE INDEX "invoices_unclaimed_email_idx" ON "invoices" USING btree ("email") WHERE "invoices"."user_id" is null;--> statement-breakpoint
CREATE INDEX "licenses_user_id_idx" ON "licenses" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "licenses_unclaimed_email_idx" ON "licenses" USING btree ("email") WHERE "licenses"."user_id" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "licenses_desktop_order_key" ON "licenses" USING btree ("order_id") WHERE "licenses"."plan" = 'desktop' and "licenses"."reissue_of" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "licenses_pro_period_key" ON "licenses" USING btree ("subscription_id","period_start") WHERE "licenses"."plan" = 'pro' and "licenses"."reissue_of" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "orders_provider_order_key" ON "orders" USING btree ("provider","provider_order_id");--> statement-breakpoint
CREATE INDEX "orders_user_id_idx" ON "orders" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "orders_unclaimed_email_idx" ON "orders" USING btree ("email") WHERE "orders"."user_id" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "subscriptions_provider_subscription_key" ON "subscriptions" USING btree ("provider","provider_subscription_id");--> statement-breakpoint
CREATE INDEX "subscriptions_user_id_idx" ON "subscriptions" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "subscriptions_unclaimed_email_idx" ON "subscriptions" USING btree ("email") WHERE "subscriptions"."user_id" is null;--> statement-breakpoint
CREATE UNIQUE INDEX "api_keys_secret_hash_key" ON "api_keys" USING btree ("secret_hash");--> statement-breakpoint
CREATE UNIQUE INDEX "api_keys_prefix_key" ON "api_keys" USING btree ("prefix");--> statement-breakpoint
CREATE INDEX "api_keys_user_id_active_idx" ON "api_keys" USING btree ("user_id") WHERE "api_keys"."revoked_at" is null;--> statement-breakpoint
CREATE INDEX "cloud_jobs_queued_idx" ON "cloud_jobs" USING btree ("queued_at") WHERE "cloud_jobs"."status" = 'queued';--> statement-breakpoint
CREATE INDEX "cloud_jobs_lease_idx" ON "cloud_jobs" USING btree ("lease_expires_at") WHERE "cloud_jobs"."status" = 'running';--> statement-breakpoint
CREATE INDEX "cloud_jobs_user_created_idx" ON "cloud_jobs" USING btree ("user_id","created_at" DESC NULLS LAST);--> statement-breakpoint
CREATE INDEX "cloud_jobs_expires_at_idx" ON "cloud_jobs" USING btree ("expires_at");--> statement-breakpoint
CREATE UNIQUE INDEX "usage_events_job_kind_key" ON "usage_events" USING btree ("job_id","kind") WHERE "usage_events"."corrects" is null;--> statement-breakpoint
CREATE INDEX "usage_events_user_occurred_idx" ON "usage_events" USING btree ("user_id","occurred_at");--> statement-breakpoint
CREATE INDEX "usage_events_unreported_idx" ON "usage_events" USING btree ("occurred_at") WHERE "usage_events"."reported_at" is null;
--> statement-breakpoint
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
grant select, update on devices, orders, subscriptions, licenses, invoices to convt_web;
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
