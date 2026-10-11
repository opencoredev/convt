-- Keep the applied 0006 trigger unchanged. Resync when any subscription
-- lifecycle field changes so paid/free attributes stay current.
drop trigger subscriptions_resync_marketing on subscriptions;
--> statement-breakpoint
create trigger subscriptions_resync_marketing after insert or update of user_id, status, current_period_end, cancel_at_period_end, trial_ends_at on subscriptions
  for each row execute function owner_resync_marketing();
