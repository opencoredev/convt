-- Down files run only in development and CI (production migrations are
-- forward-only). The columns hold per-job request metadata only, so dropping
-- them loses no financial history.
DROP INDEX "cloud_jobs_webhook_pending_idx";--> statement-breakpoint
DROP INDEX "cloud_jobs_user_idempotency_key";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_delivered_at";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_next_at";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_attempts";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_url";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "source_url";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "request_hash";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "idempotency_key";
