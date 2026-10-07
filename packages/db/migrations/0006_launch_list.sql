CREATE TABLE "launch_list" (
	"email" text PRIMARY KEY NOT NULL,
	"source" text NOT NULL,
	"unsubscribe_token_hash" text NOT NULL,
	"consented_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_requested_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "launch_list_email_normalized" CHECK ("launch_list"."email" = lower(btrim("launch_list"."email"))),
	CONSTRAINT "launch_list_source_check" CHECK ("launch_list"."source" in ('landing', 'download', 'checkout_success'))
);
--> statement-breakpoint
CREATE UNIQUE INDEX "launch_list_unsubscribe_token_hash_key" ON "launch_list" USING btree ("unsubscribe_token_hash");--> statement-breakpoint
grant select, insert, update, delete on launch_list to convt_web;
