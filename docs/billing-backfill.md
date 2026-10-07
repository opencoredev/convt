# Polar order backfill

Use this when Polar shows a paid Desktop order that our database does not have (a rejected `$0` / 100% webhook, a guest checkout before signup, or a dropped `order.paid`). It pages Polar's orders, runs each through the same ingest as a webhook, issues any missing Desktop license, and attaches rows to a verified account with that email.

Do **not** run `--apply` against production from CI or a Cloud Agent. Leo runs it by hand after a dry run.

## Dry run (default)

Needs `BILLING_DATABASE_URL` and the Polar / catalog env convt-billing uses (`POLAR_ACCESS_TOKEN`, `POLAR_API_URL` or `BILLING_CATALOG`, `LICENSE_SIGNING_KEY`, `SITE_URL`). Locally, `bun run db:up` writes those into `.convt-dev/services.env`.

```sh
bun run billing:backfill
```

Prints JSON: `scanned`, `missing` (Polar orders we have no `orders` row for), `alreadyPresent`. Writes nothing.

## Apply

```sh
bun run billing:backfill --apply
```

On the production catalog, also pass `--confirm-production` so a leftover prod token cannot write by accident:

```sh
bun run billing:backfill --apply --confirm-production
```

Safe to repeat. Already-stored orders are no-ops. Guest `$0` Desktop orders become an order, an invoice, and a license, then `claim_purchases` links them when the email is a verified account. `--apply` then drains the email outbox so the license email can send.

This Cloud Agent checkout must not run `--apply` against production.
