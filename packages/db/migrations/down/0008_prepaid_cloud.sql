-- Down files run only in development and CI (production migrations are
-- forward-only). Rows written under 0008 are converted to shapes 0007 accepts
-- rather than deleted: usage_events is append-only and references subscriptions,
-- and db:reset must work on a database that has seen a Desktop + Cloud purchase.
ALTER TABLE "subscriptions" DROP CONSTRAINT "subscriptions_prepaid_check";--> statement-breakpoint
UPDATE "subscriptions" SET "provider_subscription_id" = 'prepaid:' || "order_id" WHERE "order_id" IS NOT NULL;--> statement-breakpoint
UPDATE "orders" SET "product" = 'desktop' WHERE "product" = 'bundle';--> statement-breakpoint
UPDATE "checkouts" SET "product" = 'desktop' WHERE "product" = 'bundle';--> statement-breakpoint
ALTER TABLE "checkouts" DROP CONSTRAINT "checkouts_product_check";--> statement-breakpoint
ALTER TABLE "orders" DROP CONSTRAINT "orders_product_check";--> statement-breakpoint
DROP INDEX "subscriptions_order_key";--> statement-breakpoint
ALTER TABLE "subscriptions" DROP CONSTRAINT "subscriptions_order_id_orders_id_fk";--> statement-breakpoint
ALTER TABLE "subscriptions" DROP COLUMN "order_id";--> statement-breakpoint
ALTER TABLE "subscriptions" ALTER COLUMN "provider_subscription_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_product_check" CHECK ("orders"."product" in ('desktop'));--> statement-breakpoint
ALTER TABLE "checkouts" ADD CONSTRAINT "checkouts_product_check" CHECK ("checkouts"."product" in ('desktop', 'pro_month', 'pro_year', 'api'));
