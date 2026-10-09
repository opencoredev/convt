# Marketing email

Every convt account gets occasional product news through Sequenzy unless it unsubscribes. This page explains how consent is stored and pushed, then lists the steps to turn it on in production. Nothing here sends mail by itself: campaigns and sequences are created and sent in Sequenzy.

## How it works

Transactional email (sign-in codes, license keys, trial and payment notices) is separate and unchanged. It goes through the outbox in `packages/billing/src/outbox.ts` and Sequenzy's transactional API, and it has no unsubscribe footer.

Campaign consent lives in Postgres (`packages/db/src/schema/marketing.ts`):

- `marketing_subscriptions` holds one row per account: `subscribed` or `unsubscribed`, where that came from, and whether Sequenzy has caught up (`sync_state`).
- `marketing_consent_events` is an append-only log of every change.

Only two SQL functions change consent, and both write the log:

| Function                                                  | Called by                                                | Effect                                                                                                                             |
| --------------------------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `enroll_marketing(user, 'signup' or 'backfill')`          | the `users` insert trigger, `bun run marketing:backfill` | Subscribes an account that has no row. Never touches an existing row, so an unsubscribe survives.                                  |
| `set_marketing_consent(user, subscribed, source, detail)` | convt-billing                                            | The person's choice from Settings (`settings`) or a preferences link (`email_link`), or an opt-out Sequenzy reported (`provider`). |

Triggers mark a row `pending` again when the account's email changes or is verified, and when a purchase, refund or subscription change alters its segment attributes.

convt-billing's per-minute cron (`syncMarketing` in `packages/billing/src/marketing.ts`) pushes pending rows to Sequenzy, keyed by the convt user id as Sequenzy's `externalId`:

- Subscribed, unverified email: held until the address is verified.
- Subscribed: `PATCH /subscribers/external` with the email, first name and custom attributes, or `POST /subscribers` if no contact exists. A backfilled account is created with its signup date, which stops Sequenzy enrolling it in sequences, so existing users get no welcome email. A new signup is created without one.
- Unsubscribed: `PATCH ... {"status": "unsubscribed"}`.
- Sequenzy's status goes back to `active` only after the person subscribes again themselves.
- 429, 5xx and network errors back off (1, 5, 15, 60, 180, then 720 minutes) and raise a `marketing_sync` alert after six failures. Other 4xx answers mark the row `failed` and raise an alert. Alerts carry the user id and status code, never an address.

Each contact gets these custom attributes for templates and segments:

| Attribute        | Value                                                                                |
| ---------------- | ------------------------------------------------------------------------------------ |
| `preferencesUrl` | `https://convt.app/email/preferences?t=<token>`, signed with `MARKETING_LINK_SECRET` |
| `desktopBuyer`   | `true` when the account has an unrevoked, paid Desktop license                       |
| `proStatus`      | `active`, `trialing`, `ended` or `none`                                              |

People change their preference in three places:

- **Settings**, in the Email section on the dashboard.
- **`/email/preferences?t=...`**, the link in every campaign footer. It works signed out. Opening it changes nothing; the button POSTs, because mail scanners open links.
- **Sequenzy's own unsubscribe link**. Sequenzy reports it to `POST /webhooks/sequenzy`, which also turns `email.complained` and `email.bounced` into an unsubscribe.

Deleting an account deletes the Sequenzy contact before the user row.

## Templates

`packages/mail/src/marketing.ts` holds the campaign templates: welcome, the lifetime Desktop announcement, Pro trial onboarding, and a product update frame. Every one ends with the same footer, which uses Sequenzy's `{{unsubscribeUrl}}` and the `{{preferencesUrl}}` attribute.

Export them as HTML and text to paste into Sequenzy:

```sh
MARKETING_POSTAL_ADDRESS="..." bun run --cwd packages/mail export:marketing /tmp/convt-marketing
```

US law (CAN-SPAM) requires a postal address in commercial email. Without `MARKETING_POSTAL_ADDRESS`, the footer shows `[postal address required]` so a campaign cannot go out with a blank one.

## Turn it on in production

Do these in order. Each step is reversible until the backfill runs and mail goes out.

1. Deploy the privacy policy change. It must be live before anyone is subscribed.
2. In Sequenzy, create an API key with the `subscribers:write` scope. The existing transactional key cannot manage contacts. Create the list for product news if you want one, and note its id.
3. Create an outbound webhook to `https://convt.app/webhooks/sequenzy` with the default events (they include `subscriber.unsubscribed`, `email.bounced` and `email.complained`). Note the signing secret.
4. Set the convt-billing Worker's settings:

   | Setting                       | Kind          | Value                                                                                                               |
   | ----------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------- |
   | `SEQUENZY_MARKETING_API_KEY`  | secret        | the key from step 2                                                                                                 |
   | `MARKETING_LINK_SECRET`       | secret        | at least 32 random characters, e.g. `openssl rand -hex 32`. Rotating it breaks every preferences link already sent. |
   | `SEQUENZY_WEBHOOK_SECRET`     | secret        | the secret from step 3                                                                                              |
   | `SEQUENZY_MARKETING_LIST_IDS` | var, optional | comma-separated list ids; empty uses the workspace default lists                                                    |
   | `SEQUENZY_MARKETING_TAGS`     | var, optional | comma-separated tags for new contacts; default `convt-account`                                                      |

5. Apply migration `0006_marketing` (`bun run db:migrate` against production as usual). From then on every new account is subscribed and pushed.
6. Dry-run the backfill and read the plan. It prints user ids, never addresses:

   ```sh
   OWNER_DATABASE_URL=... bun run marketing:backfill --remote --confirm-production
   ```

7. Apply it. The cron pushes about 100 accounts a minute:

   ```sh
   OWNER_DATABASE_URL=... bun run marketing:backfill --apply --remote --confirm-production
   ```

8. Before a campaign that segments on `desktopBuyer` or `proStatus`, `--apply --resync` pushes every subscriber again to refresh the attributes.

Without `SEQUENZY_MARKETING_API_KEY`, consent is still recorded and rows wait in `pending`. Without `SEQUENZY_WEBHOOK_SECRET`, `/webhooks/sequenzy` answers 404. Without `MARKETING_LINK_SECRET`, preferences links are rejected.

## Local development

`bun run dev:web` points convt-billing at the billing mock's Sequenzy routes (`tools/billing-mock/src/sequenzy.ts`) with generated secrets, so signup, Settings and preferences links run the real push. `curl "$BILLING_MOCK_URL/admin/state"` lists the mock's contacts. Trigger a push with `curl "$origin/__billing/scheduled?cron=*+*+*+*+*"`.
