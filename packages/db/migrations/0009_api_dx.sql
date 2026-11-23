ALTER TABLE "cloud_jobs" ADD COLUMN "idempotency_key" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "request_hash" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "source_url" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_url" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_attempts" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_next_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "webhook_delivered_at" timestamp with time zone;--> statement-breakpoint
CREATE UNIQUE INDEX "cloud_jobs_user_idempotency_key" ON "cloud_jobs" USING btree ("user_id","idempotency_key") WHERE "cloud_jobs"."idempotency_key" is not null;--> statement-breakpoint
CREATE INDEX "cloud_jobs_webhook_pending_idx" ON "cloud_jobs" USING btree ("webhook_next_at") WHERE "cloud_jobs"."webhook_url" is not null and "cloud_jobs"."webhook_delivered_at" is null;