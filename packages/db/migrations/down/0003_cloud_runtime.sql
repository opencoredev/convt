revoke insert, update on api_keys from convt_web;
--> statement-breakpoint
revoke insert, update on otp_send_limits from convt_server;
--> statement-breakpoint
revoke update (updated_at) on subscriptions from convt_server, convt_web;
--> statement-breakpoint
alter table cloud_jobs drop constraint cloud_jobs_subscription_id_subscriptions_id_fk;
--> statement-breakpoint
alter table cloud_jobs drop column quota_period_start, drop column subscription_id;
