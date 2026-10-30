-- Refuses while prepaid cloud years or bundle orders exist: dropping them would
-- delete financial rows, and production migrations are forward-only.
do $$
begin
  if exists (select 1 from subscriptions where order_id is not null)
    or exists (select 1 from orders where product <> 'desktop')
    or exists (select 1 from checkouts where product = 'bundle') then
    raise exception '0008 down: bundle orders or prepaid cloud years exist';
  end if;
end
$$;
--> statement-breakpoint
ALTER TABLE "checkouts" DROP CONSTRAINT "checkouts_product_check";--> statement-breakpoint
ALTER TABLE "subscriptions" DROP CONSTRAINT "subscriptions_prepaid_check";--> statement-breakpoint
ALTER TABLE "orders" DROP CONSTRAINT "orders_product_check";--> statement-breakpoint
DROP INDEX "subscriptions_order_key";--> statement-breakpoint
ALTER TABLE "subscriptions" DROP CONSTRAINT "subscriptions_order_id_orders_id_fk";--> statement-breakpoint
ALTER TABLE "subscriptions" DROP COLUMN "order_id";--> statement-breakpoint
ALTER TABLE "subscriptions" ALTER COLUMN "provider_subscription_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_product_check" CHECK ("orders"."product" in ('desktop'));--> statement-breakpoint
ALTER TABLE "checkouts" ADD CONSTRAINT "checkouts_product_check" CHECK ("checkouts"."product" in ('desktop', 'pro_month', 'pro_year', 'api'));
