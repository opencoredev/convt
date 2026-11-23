-- Down files run only in development and CI (production migrations are
-- forward-only). The columns hold per-job request metadata only, so dropping
-- them loses no financial history.
revoke select (id, user_id, source, api_key_id, status, input_format, target_format, options, input_key, input_bytes, output_keys, attempt, max_attempts, lease_owner, lease_expires_at, reserved_bytes, reserved_cents, reservation, error_code, error_detail, queued_at, started_at, finished_at, expires_at, created_at, updated_at, subscription_id, quota_period_start, webhook_attempts, webhook_next_at, webhook_delivered_at) on cloud_jobs from convt_web, convt_billing;--> statement-breakpoint
grant select on cloud_jobs to convt_web, convt_billing;--> statement-breakpoint
DROP INDEX "cloud_jobs_webhook_pending_idx";--> statement-breakpoint
DROP INDEX "cloud_jobs_user_idempotency_key";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_delivered_at";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_next_at";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_attempts";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "webhook_url";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "source_url";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "request_hash";--> statement-breakpoint
ALTER TABLE "cloud_jobs" DROP COLUMN "idempotency_key";--> statement-breakpoint
DROP TABLE "webhook_secrets";
