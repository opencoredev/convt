-- Reverses 0006_desktop_trial. Local development only: production is forward-only.
drop table trials;
--> statement-breakpoint
drop index devices_device_hash_idx;
--> statement-breakpoint
alter table devices drop column device_hash;
