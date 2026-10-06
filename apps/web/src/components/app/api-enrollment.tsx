import { useRouter } from "@tanstack/react-router";
import { useId, useState } from "react";

import { formatDate, formatMoney } from "#/lib/format";
import type { ApiEnrollment } from "#/lib/types";
import { enrollApi, openPortal, saveSpendCap, setPlanCancel } from "#/server/billing-fns";

import { FormError } from "./form-error";
import { useNotice } from "./notice";
import { Badge, Card, PrimaryButton, SecondaryButton, TextButton, cx, focusRing } from "./ui";

// API billing on the API page. No Paper artboard exists for these states; they use
// the card, badge and button styles of the other account pages.

const suggestedCap = "20";

const labels: Record<ApiEnrollment["state"], string | null> = {
  none: null,
  pending: "PENDING",
  enrolled: "ACTIVE",
  payment_failed: "PAYMENT FAILED",
  ended: "ENDED",
};

function CapInput({
  id,
  value,
  onChange,
  describedBy,
}: {
  id: string;
  value: string;
  onChange: (v: string) => void;
  describedBy: string;
}) {
  return (
    <div className="flex h-9 w-36 items-center rounded-lg bg-page px-3 ring-1 ring-line focus-within:ring-2 focus-within:ring-green dark:bg-sunken">
      <span aria-hidden="true" className="font-mono text-[13px]/4 text-ink-2">
        $
      </span>
      <input
        id={id}
        inputMode="decimal"
        autoComplete="off"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-describedby={describedBy}
        className="w-full bg-transparent pl-1 font-mono text-[13px]/4 outline-none"
      />
    </div>
  );
}

export function ApiEnrollmentCard({
  enrollment,
  blocked,
  available,
}: {
  enrollment: ApiEnrollment;
  blocked: boolean;
  available: boolean;
}) {
  const router = useRouter();
  const notice = useNotice();
  const capId = useId();
  const hintId = useId();
  const [cap, setCap] = useState(
    enrollment.spendCapCents !== null ? String(enrollment.spendCapCents / 100) : suggestedCap,
  );
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const state = enrollment.state;
  const canEnroll = state === "none" || state === "ended";

  async function enroll() {
    setBusy(true);
    setError(null);
    try {
      const r = await enrollApi({ data: { cap } });
      if (!r.ok) {
        setError(r.message);
        setBusy(false);
        return;
      }
      window.location.assign(r.url);
    } catch {
      setError("Something went wrong. Try again.");
      setBusy(false);
    }
  }

  async function saveCap() {
    setBusy(true);
    setError(null);
    try {
      const r = await saveSpendCap({ data: { cap } });
      if (!r.ok) setError(r.message);
      else {
        setEditing(false);
        notice("Spend cap saved.");
        await router.invalidate();
      }
    } finally {
      setBusy(false);
    }
  }

  async function portal() {
    const { url } = await openPortal();
    if (url) window.location.assign(url);
    else notice("Couldn't open billing management. Try again in a minute.");
  }

  async function setEnding(cancel: boolean) {
    setBusy(true);
    try {
      const r = await setPlanCancel({ data: { kind: "api", cancel } });
      if (!r.ok) setError(r.message);
      else {
        notice(cancel ? "API billing ends with this month." : "API billing continues.");
        await router.invalidate();
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className="flex flex-col gap-4 p-6" aria-labelledby="enroll-title">
      <div className="flex flex-wrap items-center gap-2.5">
        <h2 id="enroll-title" className="text-[15px]/4.5 font-semibold">
          API billing
        </h2>
        {labels[state] ? (
          <Badge size="sm" tone={state === "enrolled" ? "green" : "neutral"}>
            {labels[state]}
          </Badge>
        ) : null}
      </div>

      {canEnroll && !available ? (
        <p className="text-[13px]/5 text-ink-2">API billing is coming soon.</p>
      ) : canEnroll && blocked ? (
        <p className="text-[13px]/5 text-ink-2">
          API billing can't start yet: our payment provider has to allow a second subscription on an
          account before an account with Pro can add the API. Nothing is charged. Check back soon.
        </p>
      ) : canEnroll ? (
        <>
          <p className="text-[13px]/5 text-ink-2">
            {state === "ended" ? "API billing ended. " : ""}Add a card to use API keys. There's no
            base fee: each conversion is billed at the end of the month, and a monthly spend cap
            stops new jobs once you reach it.
          </p>
          <div className="flex flex-wrap items-end gap-3">
            <div className="flex flex-col gap-1.5">
              <label htmlFor={capId} className="text-[13px]/4 font-medium">
                Monthly spend cap
              </label>
              <CapInput id={capId} value={cap} onChange={setCap} describedBy={hintId} />
            </div>
            <PrimaryButton disabled={busy} onClick={enroll}>
              {busy ? "Opening checkout…" : "Add card"}
            </PrimaryButton>
          </div>
          <p id={hintId} className="text-xs/4 text-ink-3">
            $20 is a good start. Anything from $1 to $10,000; you can change it later.
          </p>
        </>
      ) : state === "pending" ? (
        <p className="text-[13px]/5 text-ink-2">
          We're waiting for the payment provider to confirm your card. This page updates when it
          does; API keys work after that.
        </p>
      ) : state === "payment_failed" ? (
        <>
          <p className="text-[13px]/5 text-ink-2">
            Last month's API usage couldn't be charged. Your keys still exist, but new jobs are
            refused until the payment goes through.
          </p>
          <div>
            <PrimaryButton onClick={portal}>Update card</PrimaryButton>
          </div>
        </>
      ) : (
        <>
          <p className="text-[13px]/5 text-ink-2">
            Spend cap{" "}
            <span className="font-mono text-ink">{formatMoney(enrollment.spendCapCents ?? 0)}</span>{" "}
            a month. New jobs are refused once a month's usage would pass it.
            {enrollment.endsOn ? ` Billing ends ${formatDate(enrollment.endsOn)}.` : ""}
          </p>
          {editing ? (
            <div className="flex flex-wrap items-end gap-3">
              <div className="flex flex-col gap-1.5">
                <label htmlFor={capId} className="text-[13px]/4 font-medium">
                  New monthly cap
                </label>
                <CapInput id={capId} value={cap} onChange={setCap} describedBy={hintId} />
              </div>
              <PrimaryButton disabled={busy} onClick={saveCap}>
                {busy ? "Saving…" : "Save cap"}
              </PrimaryButton>
              <TextButton tone="muted" onClick={() => setEditing(false)}>
                Cancel
              </TextButton>
              <p id={hintId} className="w-full text-xs/4 text-ink-3">
                Lowering the cap keeps jobs already running; it stops new ones above it.
              </p>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              <SecondaryButton onClick={() => setEditing(true)}>Change cap</SecondaryButton>
              {enrollment.endsOn ? (
                <TextButton disabled={busy} onClick={() => setEnding(false)}>
                  Keep API billing
                </TextButton>
              ) : (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setEnding(true)}
                  className={cx(
                    "cursor-pointer rounded-sm text-[13px]/4 text-ink-2 hover:text-ink",
                    focusRing,
                  )}
                >
                  End API billing
                </button>
              )}
            </div>
          )}
        </>
      )}
      <FormError>{error}</FormError>
    </Card>
  );
}
