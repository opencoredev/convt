// "convt runs on your computer": what a phone visitor gets instead of a download.
// It takes an email address and sends the download link there (server function in
// src/server/mobile-link-fns.ts). Every card on a page shares one form state, so
// after a send the dialog and the inline card on /download both say it was sent.
//
// Direction: the /download "For your computer" note card (raised surface, shadow-note,
// mono eyebrow), mirrored as "On your phone"; on a phone it opens as a bottom sheet in
// the thumb zone. No new colors: the site's ink, green button and error tokens.

import posthog from "posthog-js";
import { useEffect, useId, useRef, useState, useSyncExternalStore, type MouseEvent } from "react";

import { cx, focusRing } from "#/components/app/ui";
import { activeOffer } from "#/lib/launch-offer";
import { isMobileBrowser, normalizeEmail, type CaptureSource } from "#/lib/mobile";
import { sendMobileDownloadLink } from "#/server/mobile-link-fns";

type FormState =
  | { status: "idle" }
  | { status: "sending" }
  | { status: "sent" }
  | { status: "error"; message: string };

const errorText = {
  invalid_email: "That doesn't look like an email address. Check it and try again.",
  too_many: "Too many tries. Wait a while, then try again.",
  send_failed: "We couldn't send the email. Try again in a minute.",
} as const;

// One state per page, shared by every card.
let formState: FormState = { status: "idle" };
const listeners = new Set<() => void>();
function setFormState(next: FormState) {
  formState = next;
  for (const l of listeners) l();
}
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};
const idle: FormState = { status: "idle" };
const useFormState = () =>
  useSyncExternalStore(
    subscribe,
    () => formState,
    () => idle,
  );
const noop = () => () => {};
/** False in the server's HTML and during hydration, true once React handles the form. */
const useHydrated = () =>
  useSyncExternalStore(
    noop,
    () => true,
    () => false,
  );

async function submit(email: string, source: CaptureSource) {
  if (formState.status === "sending") return;
  if (!normalizeEmail(email))
    return setFormState({ status: "error", message: errorText.invalid_email });
  setFormState({ status: "sending" });
  try {
    const result = await sendMobileDownloadLink({ data: { email, source } });
    if (!result.ok) return setFormState({ status: "error", message: errorText[result.error] });
    setFormState({ status: "sent" });
    // Only where the capture started: never the address.
    if (posthog.__loaded) posthog.capture("mobile_email_capture_submitted", { source });
  } catch {
    setFormState({ status: "error", message: errorText.send_failed });
  }
}

/** The card itself: heading, one email field, and its sent and error states. */
export function MobileEmailCard({
  source,
  className,
  autoFocus,
  headingId,
}: {
  source: CaptureSource;
  className?: string;
  autoFocus?: boolean;
  headingId?: string;
}) {
  const state = useFormState();
  const hydrated = useHydrated();
  const ownId = useId();
  const titleId = headingId ?? `${ownId}-title`;
  const inputId = `${ownId}-email`;
  const errorId = `${ownId}-error`;
  const offer = activeOffer(new Date());
  const error = state.status === "error" ? state.message : null;

  return (
    <section
      aria-labelledby={titleId}
      data-testid="mobile-email-card"
      className={cx("flex flex-col gap-5 rounded-xl bg-raised p-5 shadow-note", className)}
    >
      <div className="flex flex-col gap-1.5">
        <p className="font-mono text-[11px]/3.5 text-ink-2 uppercase">On your phone</p>
        <h2 id={titleId} className="text-[22px]/7 font-semibold tracking-[-0.02em] text-ink">
          convt runs on your computer
        </h2>
        <p className="text-[15px]/[22px] text-ink-2">
          {offer
            ? "You're on your phone. Drop your email and we'll send the download link, plus a launch discount, so it's waiting when you're at your desk."
            : "You're on your phone. Drop your email and we'll send the download link, so it's waiting when you're at your desk."}
        </p>
      </div>
      <div aria-live="polite">
        {state.status === "sent" ? (
          <p
            data-testid="mobile-email-sent"
            className="flex items-center gap-2.5 rounded-lg bg-sunken px-3.5 py-3 text-[15px]/[22px] font-medium text-ink shadow-[inset_0_0_0_1px_var(--line)]"
          >
            <CheckIcon />
            Sent. Check your inbox on your computer.
          </p>
        ) : (
          <form
            noValidate
            className="flex flex-col gap-2.5"
            onSubmit={(event) => {
              event.preventDefault();
              const email = new FormData(event.currentTarget).get("email");
              void submit(typeof email === "string" ? email : "", source);
            }}
          >
            <label htmlFor={inputId} className="sr-only">
              Email
            </label>
            <input
              id={inputId}
              type="email"
              name="email"
              required
              autoComplete="email"
              inputMode="email"
              autoCapitalize="none"
              spellCheck={false}
              maxLength={254}
              // The dialog opens on a tap meant to download; going straight to the field saves one.
              autoFocus={autoFocus}
              placeholder="you@email.com"
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? errorId : undefined}
              className="h-11 rounded-lg bg-page px-3 text-base/5 text-ink shadow-input outline-none placeholder:text-ink-3 focus-visible:ring-2 focus-visible:ring-green dark:bg-sunken"
            />
            {error ? (
              <p id={errorId} role="alert" className="text-[13px]/[18px] text-error">
                {error}
              </p>
            ) : null}
            <button
              type="submit"
              // Until hydration a submit would be a native GET with the address in the URL.
              disabled={!hydrated || state.status === "sending"}
              className={cx(
                "btn-primary h-11 cursor-pointer rounded-lg text-[15px]/5 font-medium disabled:cursor-wait disabled:opacity-70",
                focusRing,
              )}
            >
              {state.status === "sending" ? "Sending…" : "Send me the link"}
            </button>
          </form>
        )}
      </div>
    </section>
  );
}

function CheckIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true" className="shrink-0">
      <circle cx="12" cy="12" r="10" className="fill-green" />
      <path
        d="m7.5 12.5 3 3 6-6.5"
        fill="none"
        stroke="#ffffff"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** The card as a modal: a bottom sheet on phones, centered on wider screens. */
function MobileDownloadDialog({ source, onClose }: { source: CaptureSource; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onClose={onClose}
      // A tap on the backdrop (the dialog element itself, outside the card) closes it.
      onClick={(event) => {
        if (event.target === event.currentTarget) event.currentTarget.close();
      }}
      className="m-0 mt-auto w-full max-w-none bg-transparent p-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] text-left backdrop:bg-black/40 motion-safe:transition-[translate,opacity] motion-safe:duration-200 motion-safe:ease-out sm:m-auto sm:max-w-[440px] starting:open:translate-y-3 starting:open:opacity-0"
    >
      <div className="relative">
        <MobileEmailCard source={source} headingId={titleId} autoFocus className="pr-12" />
        <button
          type="button"
          aria-label="Close"
          onClick={() => ref.current?.close()}
          className={cx(
            "absolute top-3 right-3 flex size-9 cursor-pointer items-center justify-center rounded-lg text-ink-2 hover:bg-hover hover:text-ink",
            focusRing,
          )}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M6 6l12 12M18 6 6 18"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            />
          </svg>
        </button>
      </div>
    </dialog>
  );
}

/**
 * For a Download link or button: on a phone or tablet the tap opens the card instead
 * of following the link. Pass `onClick` to the link and render `dialog` next to it.
 * Before hydration the link works as usual and lands on /download, which shows the card.
 */
export function useMobileDownloadIntercept(source: CaptureSource) {
  const [open, setOpen] = useState(false);
  const onClick = (event: MouseEvent<HTMLElement>) => {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.shiftKey) return;
    if (!isMobileBrowser()) return;
    event.preventDefault();
    setOpen(true);
  };
  const dialog = open ? (
    <MobileDownloadDialog source={source} onClose={() => setOpen(false)} />
  ) : null;
  return { onClick, dialog };
}

/**
 * The card in a page's flow, for a visitor whose user agent the server already knows is
 * a phone or tablet. CSS shows it only at mobile widths (`mobileMaxWidth` in
 * src/lib/mobile.ts), so the server's HTML is already right and nothing swaps on load.
 */
export function MobileEmailNote({ source }: { source: CaptureSource }) {
  return (
    <div className="hidden max-[1024px]:block">
      <MobileEmailCard source={source} />
    </div>
  );
}
