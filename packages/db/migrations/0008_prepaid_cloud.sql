ALTER TABLE "orders" DROP CONSTRAINT "orders_product_check";--> statement-breakpoint
ALTER TABLE "checkouts" DROP CONSTRAINT "checkouts_product_check";--> statement-breakpoint
ALTER TABLE "subscriptions" ALTER COLUMN "provider_subscription_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD COLUMN "order_id" text;--> statement-breakpoint
ALTER TABLE "subscriptions" ADD CONSTRAINT "subscriptions_order_id_orders_id_fk" FOREIGN KEY ("order_id") REFERENCES "public"."orders"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "subscriptions_order_key" ON "subscriptions" USING btree ("order_id") WHERE "subscriptions"."order_id" is not null;--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_product_check" CHECK ("orders"."product" in ('desktop', 'bundle'));--> statement-breakpoint
ALTER TABLE "subscriptions" ADD CONSTRAINT "subscriptions_prepaid_check" CHECK (("subscriptions"."order_id" is null) = ("subscriptions"."provider_subscription_id" is not null) and ("subscriptions"."order_id" is null or ("subscriptions"."kind" = 'pro' and "subscriptions"."interval" = 'year' and "subscriptions"."cancel_at_period_end" and "subscriptions"."current_period_start" is not null and "subscriptions"."current_period_end" is not null and "subscriptions"."ended_at" is not null)));--> statement-breakpoint
ALTER TABLE "checkouts" ADD CONSTRAINT "checkouts_product_check" CHECK ("checkouts"."product" in ('desktop', 'bundle', 'pro_month', 'pro_year', 'api'));