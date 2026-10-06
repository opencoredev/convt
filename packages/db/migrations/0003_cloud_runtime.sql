ALTER TABLE "cloud_jobs" ADD COLUMN "subscription_id" text;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD COLUMN "quota_period_start" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "cloud_jobs" ADD CONSTRAINT "cloud_jobs_subscription_id_subscriptions_id_fk" FOREIGN KEY ("subscription_id") REFERENCES "public"."subscriptions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
grant insert, update on api_keys to convt_web;
--> statement-breakpoint
grant insert, update on otp_send_limits to convt_server;
--> statement-breakpoint
grant update (updated_at) on subscriptions to convt_server, convt_web;
