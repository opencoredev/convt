import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useId, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { usePlaceholderAction } from "#/components/app/notice";
import { cx, focusRing } from "#/components/app/ui";

export const Route = createFileRoute("/_app/sign-in/")({
  validateSearch: (search: Record<string, unknown>): { email?: string } =>
    typeof search.email === "string" ? { email: search.email } : {},
  head: () => ({ meta: [{ title: "Sign in · convt" }] }),
  component: SignInPage,
});

// Sign-in providers. Buttons are text-only by Leo's decision.
const providers = ["GitHub", "Google", "Apple"] as const;

function SignInPage() {
  const { email: initialEmail } = Route.useSearch();
  const navigate = useNavigate();
  const placeholder = usePlaceholderAction();
  const emailId = useId();
  const hintId = useId();
  const [email, setEmail] = useState(initialEmail ?? "");

  return (
    <AuthLayout>
      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Sign in to convt</h1>
        <p className="text-sm/[22px] text-ink-2">
          Use your email or an account you already have. New here? Signing in creates your account.
        </p>
      </div>

      <form
        className="flex flex-col gap-6"
        onSubmit={(event) => {
          event.preventDefault();
          // PLACEHOLDER FLOW: no email is sent. There is no auth backend yet (plan P6), so
          // this says so and shows the next screen as a preview. Replace with the
          // magic-link request.
          placeholder("Sending sign-in emails");
          void navigate({ to: "/sign-in/check-email", search: { email: email.trim() } });
        }}
      >
        <div className="flex flex-col gap-2">
          <label htmlFor={emailId} className="text-[13px]/4 font-medium">
            Email
          </label>
          <input
            id={emailId}
            type="email"
            name="email"
            required
            autoComplete="email"
            inputMode="email"
            placeholder="you@example.com"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            aria-describedby={hintId}
            className="h-10 rounded-lg bg-raised px-3 text-sm/4.5 text-ink shadow-input outline-none placeholder:text-ink-3 focus-visible:ring-2 focus-visible:ring-green"
          />
        </div>
        <button
          type="submit"
          className={cx(
            "btn-primary h-10 cursor-pointer rounded-lg text-sm/4.5 font-medium",
            focusRing,
          )}
        >
          Email me a link
        </button>
      </form>

      <div className="flex flex-col gap-2.5">
        <div className="flex items-center gap-3">
          <span className="h-px flex-1 bg-line" />
          <span className="text-xs/4 text-ink-3">or continue with</span>
          <span className="h-px flex-1 bg-line" />
        </div>
        <div className="flex gap-2">
          {providers.map((provider) => (
            <button
              key={provider}
              type="button"
              // PLACEHOLDER: GitHub, Google and Apple sign-in are not set up yet.
              onClick={() => placeholder(`Signing in with ${provider}`)}
              className={cx(
                "h-10 flex-1 cursor-pointer rounded-lg bg-raised text-[13px]/4 font-medium shadow-button hover:bg-hover dark:bg-sunken",
                focusRing,
              )}
            >
              {provider}
            </button>
          ))}
        </div>
      </div>

      <p id={hintId} className="text-xs/[18px] text-ink-3">
        Bought a license without an account? Use the email from your receipt and it will be waiting
        for you.
      </p>
    </AuthLayout>
  );
}
