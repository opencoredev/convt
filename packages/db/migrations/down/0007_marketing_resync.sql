drop trigger subscriptions_resync_marketing on subscriptions;
--> statement-breakpoint
create trigger subscriptions_resync_marketing after insert or update of user_id, status on subscriptions
  for each row execute function owner_resync_marketing();
