CREATE TABLE "trials" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text,
	"device_hash" text,
	"started_at" timestamp with time zone NOT NULL,
	"ends_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "trials_ends_after_start" CHECK ("trials"."ends_at" > "trials"."started_at"),
	CONSTRAINT "trials_device_hash_format" CHECK ("trials"."device_hash" ~ '^[0-9a-f]{64}$')
);
--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "device_hash" text;--> statement-breakpoint
ALTER TABLE "trials" ADD CONSTRAINT "trials_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "trials_user_id_key" ON "trials" USING btree ("user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "trials_device_hash_key" ON "trials" USING btree ("device_hash");--> statement-breakpoint
CREATE INDEX "devices_device_hash_idx" ON "devices" USING btree ("device_hash");--> statement-breakpoint
-- CNV-56 desktop trial: only convt-billing creates and reads trials. The site writes
-- devices.device_hash through its existing update grant on devices.
grant select, insert on trials to convt_billing;
--> statement-breakpoint
-- startTrial reads which computers got an account's trial.
grant select (user_id, device_hash) on devices to convt_billing;
