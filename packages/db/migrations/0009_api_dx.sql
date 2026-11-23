-- Fail fast instead of queueing behind long transactions: the ALTERs and index
-- builds hold cloud_jobs' lock, and the worker queue must not stall behind them.
SET LOCAL lock_timeout = '5s';--> statement-breakpoint
CREATE TABLE "webhook_secrets" (
	"user_id" text PRIMARY KEY NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "idempotency_key" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "request_hash" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "source_url" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_url" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_attempts" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_next_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_delivered_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "webhook_secrets" ADD CONSTRAINT "webhook_secrets_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX IF NOT EXISTS "cloud_jobs_user_idempotency_key" ON "cloud_jobs" USING btree ("user_id","idempotency_key") WHERE "cloud_jobs"."idempotency_key" is not null;--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "cloud_jobs_webhook_pending_idx" ON "cloud_jobs" USING btree ("webhook_next_at") WHERE "cloud_jobs"."webhook_url" is not null and "cloud_jobs"."webhook_delivered_at" is null;--> statement-breakpoint
grant select, insert, update on webhook_secrets to convt_server;
