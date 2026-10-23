import { useId, useRef, useState } from "react";
import { useRouterState } from "@tanstack/react-router";

import { cx, focusRing, PrimaryButton, SecondaryButton } from "#/components/app/ui";
import { FormError } from "#/components/app/form-error";
import { SUPPORT_EMAIL } from "#/lib/site";

/** Matches feedbackLimits.message on the server. */
const MAX_MESSAGE = 4000;

type Status = { kind: "editing"; error: string | null } | { kind: "sending" } | { kind: "sent" };

const errors: Record<string, string> = {
  invalid_message: "Write a message first.",
  invalid_email: "That email address doesn't look right.",
  rate_limited: "That's a lot of feedback at once. Try again in an hour.",
  unavailable: `Feedback isn't set up yet. Email ${SUPPORT_EMAIL} instead.`,
};

function browserPlatform(): string | null {
  const nav: Navigator & { userAgentData?: { platform?: string } } = navigator;
  return nav.userAgentData?.platform || nav.platform || null;
}

const field =
  "rounded-lg bg-raised px-3 text-sm/5 text-ink shadow-input outline-none placeholder:text-ink-3 focus-visible:ring-2 focus-visible:ring-green";

/**
 * The Feedback button in the corner of every page, and the dialog it opens.
 * Submissions go to /api/feedback, which sends them to PostHog as a survey response.
 */
export function FeedbackButton() {
  const dialog = useRef<HTMLDialogElement>(null);
  const path = useRouterState({ select: (s) => s.location.pathname });
  const [message, setMessage] = useState("");
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "editing", error: null });
  const titleId = useId();
  const messageId = useId();
  const emailId = useId();
  const noteId = useId();

  const open = () => {
    if (status.kind === "sent") {
      setMessage("");
      setStatus({ kind: "editing", error: null });
    }
    dialog.current?.showModal();
  };
  const close = () => dialog.current?.close();

  const send = async () => {
    if (message.trim() === "") {
      setStatus({ kind: "editing", error: errors.invalid_message });
      return;
    }
    setStatus({ kind: "sending" });
    try {
      const response = await fetch("/api/feedback", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          source: "web",
          message,
          email: email.trim() || null,
          platform: browserPlatform(),
          page: path,
        }),
      });
      if (response.ok) {
        setStatus({ kind: "sent" });
        return;
      }
      const body: unknown = await response.json().catch(() => null);
      const code =
        body !== null && typeof body === "object" && "error" in body ? String(body.error) : "";
      setStatus({
        kind: "editing",
        error: errors[code] ?? "Your feedback didn't send. Try again in a moment.",
      });
    } catch {
      setStatus({ kind: "editing", error: "You look offline. Your message is still here." });
    }
  };

  const error = status.kind === "editing" ? status.error : null;

  return (
    <>
      <button
        type="button"
        onClick={open}
        aria-haspopup="dialog"
        className={cx(
          "fixed right-4 bottom-4 z-40 inline-flex cursor-pointer items-center gap-1.5 rounded-full bg-raised px-3.5 py-2 text-[13px]/4 font-medium text-ink shadow-float transition-colors hover:bg-hover dark:bg-sunken",
          focusRing,
        )}
      >
        <FeedbackIcon />
        Feedback
      </button>
      <dialog
        ref={dialog}
        aria-labelledby={titleId}
        onClick={(event) => {
          // A click on the backdrop lands on the dialog element itself.
          if (event.target === event.currentTarget) close();
        }}
        className="m-auto w-[calc(100%-2rem)] max-w-[440px] rounded-xl bg-page p-0 text-ink shadow-note backdrop:bg-black/40"
      >
        {status.kind === "sent" ? (
          <div className="flex flex-col gap-4 p-5">
            <h2 id={titleId} className="text-[17px]/[22px] font-semibold tracking-[-0.01em]">
              Thanks for the feedback
            </h2>
            <p role="status" className="text-sm/[21px] text-ink-2">
              {email.trim()
                ? "We read every message, and we'll reply by email if we have a question."
                : "We read every message."}
            </p>
            <div className="flex justify-end">
              <PrimaryButton onClick={close} autoFocus>
                Done
              </PrimaryButton>
            </div>
          </div>
        ) : (
          <form
            className="flex flex-col gap-4 p-5"
            onSubmit={(event) => {
              event.preventDefault();
              void send();
            }}
          >
            <div className="flex flex-col gap-1">
              <h2 id={titleId} className="text-[17px]/[22px] font-semibold tracking-[-0.01em]">
                Send feedback
              </h2>
              <p id={noteId} className="text-[13px]/[18px] text-ink-2">
                A bug, a format you need, anything. It goes straight to the convt team.
              </p>
            </div>
            <div className="flex flex-col gap-2">
              <label htmlFor={messageId} className="text-[13px]/4 font-medium">
                Message
              </label>
              <textarea
                id={messageId}
                required
                rows={5}
                maxLength={MAX_MESSAGE}
                value={message}
                onChange={(event) => setMessage(event.target.value)}
                aria-describedby={noteId}
                aria-invalid={status.kind === "editing" && status.error === errors.invalid_message}
                className={cx(field, "min-h-28 resize-y py-2.5")}
              />
            </div>
            <div className="flex flex-col gap-2">
              <label htmlFor={emailId} className="text-[13px]/4 font-medium">
                Email{" "}
                <span className="font-normal text-ink-2">(optional, if you'd like a reply)</span>
              </label>
              <input
                id={emailId}
                type="email"
                autoComplete="email"
                inputMode="email"
                placeholder="you@example.com"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                className={cx(field, "h-10")}
              />
            </div>
            <FormError>{error}</FormError>
            <div className="flex items-center justify-end gap-2">
              <SecondaryButton onClick={close}>Cancel</SecondaryButton>
              <PrimaryButton type="submit" disabled={status.kind === "sending"}>
                {status.kind === "sending" ? "Sending…" : "Send feedback"}
              </PrimaryButton>
            </div>
          </form>
        )}
      </dialog>
    </>
  );
}

function FeedbackIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M2.75 3.75a1 1 0 0 1 1-1h8.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H7l-3 2.5v-2.5h-.25a1 1 0 0 1-1-1v-6Z"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinejoin="round"
      />
    </svg>
  );
}
