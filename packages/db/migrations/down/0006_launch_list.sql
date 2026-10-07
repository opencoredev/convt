-- Reverses 0006_launch_list. Local development only: production is forward-only.
revoke select, insert, update, delete on launch_list from convt_web;
--> statement-breakpoint
drop table launch_list;
