import { Link, createFileRoute } from "@tanstack/react-router";
import { useId } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { CodeInput } from "#/components/app/code-input";
import { usePlaceholderAction } from "#/components/app/notice";
import { PreviewNote, TextButton, cx, focusRing } from "#/components/app/ui";
import { magicLinkMinutes, signInCodeLength } from "#/lib/config";

// PLACEHOLDER FLOW: the sign-in form navigates here without sending anything. When
// auth exists (plan P6), this page shows after the magic-link request succeeds.
export const Route = createFileRoute("/_app/sign-in/check-email")({
  validateSearch: (search: Record<string, unknown>): { email?: string } =>
    typeof search.email === "string" && search.email !== "" ? { email: search.email } : {},
  head: () => ({ meta: [{ title: "Check your email · convt" }] }),
  component: CheckEmailPage,
});

function CheckEmailPage() {
  const { email } = Route.useSearch();
  const placeholder = usePlaceholderAction();
  const codeLabelId = useId();

  return (
    <AuthLayout>
      {/* PLACEHOLDER: the sign-in form does not send email yet; say so on this screen. */}
      <PreviewNote className="self-start">Preview only. No email was sent.</PreviewNote>
      <span
        aria-hidden="true"
        className="flex size-11 shrink-0 items-center justify-center rounded-[10px] bg-green-tint shadow-[inset_0_0_0_1px_var(--green-line)]"
      >
        <svg width="22" height="22" viewBox="0 0 24 24" className="text-green">
          <rect
            x="3"
            y="5"
            width="18"
            height="14"
            rx="2"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
          />
          <path
            d="M 3.5 6.5 L 12 13 L 20.5 6.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>

      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Check your email</h1>
        <p className="text-sm/[22px] break-words text-ink-2">
          We sent a sign-in link to {email ?? "your email"}. It works once and expires in{" "}
          {magicLinkMinutes} minutes.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <p id={codeLabelId} className="text-[13px]/4 font-medium">
          Or enter the code from the email
        </p>
        <CodeInput
          length={signInCodeLength}
          labelId={codeLabelId}
          // PLACEHOLDER: codes are checked by the auth backend, which does not exist yet.
          onComplete={() => placeholder("Checking sign-in codes")}
        />
      </div>

      <div className="flex flex-wrap gap-4">
        <TextButton onClick={() => placeholder("Resending the link")}>Resend link</TextButton>
        <Link
          to="/sign-in"
          search={email ? { email } : {}}
          className={cx("rounded-sm text-[13px]/4 text-ink-2 hover:text-ink", focusRing)}
        >
          Use a different email
        </Link>
      </div>
    </AuthLayout>
  );
}
