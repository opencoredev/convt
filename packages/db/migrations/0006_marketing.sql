CREATE TABLE "marketing_consent_events" (
	"id" bigint PRIMARY KEY GENERATED ALWAYS AS IDENTITY (sequence name "marketing_consent_events_id_seq" INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1),
	"user_id" text NOT NULL,
	"status" text NOT NULL,
	"source" text NOT NULL,
	"detail" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_consent_events_status" CHECK ("marketing_consent_events"."status" in ('subscribed', 'unsubscribed')),
	CONSTRAINT "marketing_consent_events_source" CHECK ("marketing_consent_events"."source" in ('signup', 'backfill', 'settings', 'email_link', 'provider'))
);
--> statement-breakpoint
CREATE TABLE "marketing_subscriptions" (
	"user_id" text PRIMARY KEY NOT NULL,
	"status" text NOT NULL,
	"source" text NOT NULL,
	"status_changed_at" timestamp with time zone DEFAULT now() NOT NULL,
	"reactivate" boolean DEFAULT false NOT NULL,
	"sync_state" text DEFAULT 'pending' NOT NULL,
	"sync_attempts" integer DEFAULT 0 NOT NULL,
	"next_sync_at" timestamp with time zone DEFAULT now() NOT NULL,
	"sync_lease_until" timestamp with time zone,
	"synced_at" timestamp with time zone,
	"last_error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "marketing_subscriptions_status" CHECK ("marketing_subscriptions"."status" in ('subscribed', 'unsubscribed')),
	CONSTRAINT "marketing_subscriptions_source" CHECK ("marketing_subscriptions"."source" in ('signup', 'backfill', 'settings', 'email_link', 'provider')),
	CONSTRAINT "marketing_subscriptions_sync_state" CHECK ("marketing_subscriptions"."sync_state" in ('pending', 'held', 'synced', 'failed'))
);
--> statement-breakpoint
ALTER TABLE "marketing_consent_events" ADD CONSTRAINT "marketing_consent_events_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "marketing_subscriptions" ADD CONSTRAINT "marketing_subscriptions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "marketing_consent_events_user_id_idx" ON "marketing_consent_events" USING btree ("user_id","created_at");--> statement-breakpoint
CREATE INDEX "marketing_subscriptions_due_idx" ON "marketing_subscriptions" USING btree ("next_sync_at") WHERE "marketing_subscriptions"."sync_state" = 'pending';
--> statement-breakpoint
-- Marketing email (migration 0006). convt-billing pushes consent to Sequenzy and
-- changes it for the site through its RPC; the site has no grants here. Consent
-- changes only through the two functions below, which also write the event log.
grant select, update (sync_state, sync_attempts, next_sync_at, sync_lease_until, synced_at, last_error,
  reactivate, updated_at) on marketing_subscriptions to convt_billing;
--> statement-breakpoint
grant select on marketing_consent_events to convt_billing;
--> statement-breakpoint
create function marketing_consent_events_guard() returns trigger
language plpgsql as $$
begin
  raise exception 'marketing_consent_events rows are append-only'
    using errcode = 'insufficient_privilege';
end
$$;
--> statement-breakpoint
create trigger marketing_consent_events_no_rewrite before update on marketing_consent_events
  for each row execute function marketing_consent_events_guard();
--> statement-breakpoint
-- Subscribes an account that has never had a marketing row (a new signup or the
-- backfill). An account that already has one, subscribed or not, is left alone,
-- so neither path can undo an unsubscribe. Returns true when it added the row.
create function enroll_marketing(p_user_id text, p_source text)
returns boolean
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  if p_source not in ('signup', 'backfill') then
    raise exception 'enroll_marketing: source % is not an enrollment', p_source
      using errcode = 'P0001';
  end if;
  insert into marketing_subscriptions (user_id, status, source)
    select u.id, 'subscribed', p_source from users u where u.id = p_user_id
    on conflict (user_id) do nothing;
  if not found then
    return false;
  end if;
  insert into marketing_consent_events (user_id, status, source)
    values (p_user_id, 'subscribed', p_source);
  return true;
end
$$;
--> statement-breakpoint
-- The person's own choice (`settings`, `email_link`) or an unsubscribe Sequenzy
-- reported (`provider`). Every change is pushed, a provider one too: a push of
-- an older state may still be landing at Sequenzy, and pushing the unsubscribe
-- again makes this row the final word. Subscribing again by choice sets
-- `reactivate`, the only way the push may make Sequenzy's contact active again.
-- Returns true when the status changed.
create function set_marketing_consent(p_user_id text, p_subscribed boolean, p_source text, p_detail text)
returns boolean
language plpgsql security definer set search_path = public, pg_temp as $$
declare
  v_status text := case when p_subscribed then 'subscribed' else 'unsubscribed' end;
  v_old text;
begin
  if p_source not in ('settings', 'email_link', 'provider') then
    raise exception 'set_marketing_consent: source % is not a choice', p_source
      using errcode = 'P0001';
  end if;
  if p_source = 'provider' and p_subscribed then
    raise exception 'set_marketing_consent: the provider never subscribes anyone'
      using errcode = 'P0001';
  end if;
  perform 1 from users u where u.id = p_user_id for share;
  if not found then
    return false;
  end if;
  select m.status into v_old from marketing_subscriptions m where m.user_id = p_user_id for update;
  if v_old is not distinct from v_status then
    return false;
  end if;
  insert into marketing_subscriptions (user_id, status, source, status_changed_at, reactivate)
    values (p_user_id, v_status, p_source, now(), p_subscribed)
    on conflict (user_id) do update set
      status = excluded.status, source = excluded.source, status_changed_at = excluded.status_changed_at,
      reactivate = excluded.reactivate, sync_state = 'pending', sync_attempts = 0, next_sync_at = now(),
      last_error = null, updated_at = now();
  insert into marketing_consent_events (user_id, status, source, detail)
    values (p_user_id, v_status, p_source, left(p_detail, 200));
  return true;
end
$$;
--> statement-breakpoint
revoke all on function enroll_marketing(text, text) from public;
--> statement-breakpoint
revoke all on function set_marketing_consent(text, boolean, text, text) from public;
--> statement-breakpoint
grant execute on function set_marketing_consent(text, boolean, text, text) to convt_billing;
--> statement-breakpoint
-- Every new account is subscribed (Leo's decision, 2026-10-09); the privacy policy
-- says so and every campaign email carries an unsubscribe link. Runs for whichever
-- role inserts the user.
create function users_enroll_marketing() returns trigger
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  perform enroll_marketing(new.id, 'signup');
  return new;
end
$$;
--> statement-breakpoint
create trigger users_enroll_marketing after insert on users
  for each row execute function users_enroll_marketing();
--> statement-breakpoint
-- A new address or a newly verified one has to reach Sequenzy.
create function users_resync_marketing() returns trigger
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  update marketing_subscriptions
    set sync_state = 'pending', sync_attempts = 0, next_sync_at = now(), last_error = null,
      updated_at = now()
    where user_id = new.id;
  return new;
end
$$;
--> statement-breakpoint
create trigger users_resync_marketing after update of email, email_verified on users
  for each row
  when (old.email is distinct from new.email or old.email_verified is distinct from new.email_verified)
  execute function users_resync_marketing();
--> statement-breakpoint
-- A purchase, refund or subscription change alters the attributes campaigns are
-- segmented on (Desktop buyer, Pro status), so the contact is pushed again.
create function owner_resync_marketing() returns trigger
language plpgsql security definer set search_path = public, pg_temp as $$
begin
  if new.user_id is not null then
    update marketing_subscriptions
      set sync_state = 'pending', sync_attempts = 0, next_sync_at = now(), last_error = null,
        updated_at = now()
      where user_id = new.user_id and status = 'subscribed';
  end if;
  return new;
end
$$;
--> statement-breakpoint
create trigger licenses_resync_marketing after insert or update of user_id, revoked_at on licenses
  for each row execute function owner_resync_marketing();
--> statement-breakpoint
create trigger subscriptions_resync_marketing after insert or update of user_id, status on subscriptions
  for each row execute function owner_resync_marketing();
