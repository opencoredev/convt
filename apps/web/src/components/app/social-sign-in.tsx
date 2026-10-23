import { cx, focusRing } from "#/components/app/ui";

import { GitHubMark, GoogleMark, OrDivider, darkPill } from "./auth-screen";

// Sign-in providers. Each one renders only when the Worker has it configured. Google is
// the dark pill above the email step; GitHub and Apple are quiet text links under it.
// Apple waits for the Apple Developer account (CNV-20), so it stays hidden until then.
const providers = [
  { id: "github", label: "GitHub" },
  { id: "google", label: "Google" },
  { id: "apple", label: "Apple" },
] as const;

export type SocialProviderId = (typeof providers)[number]["id"];

/** Continue with Google and the "or" under it; nothing when Google is not configured. */
export function GoogleSignIn({
  available,
  onSelect,
  busy,
}: {
  available: Record<SocialProviderId, boolean>;
  onSelect: (provider: SocialProviderId) => void;
  busy?: boolean;
}) {
  if (!available.google) return null;
  return (
    <div className="flex w-full flex-col gap-5">
      <button type="button" disabled={busy} onClick={() => onSelect("google")} className={darkPill}>
        <GoogleMark />
        Continue with Google
      </button>
      <OrDivider />
    </div>
  );
}

/** "Or continue with GitHub" as small links; nothing when no other provider is configured. */
export function OtherSignIn({
  available,
  onSelect,
}: {
  available: Record<SocialProviderId, boolean>;
  onSelect: (provider: SocialProviderId) => void;
}) {
  const shown = providers.filter((p) => p.id !== "google" && available[p.id]);
  if (shown.length === 0) return null;
  return (
    <p className="flex flex-wrap items-center justify-center gap-x-1.5 gap-y-1 text-[13px]/5 text-ink-2">
      <span>Or continue with</span>
      {shown.map((provider, i) => (
        <span key={provider.id} className="inline-flex items-center gap-1.5">
          {i > 0 ? <span aria-hidden="true">·</span> : null}
          <button
            type="button"
            onClick={() => onSelect(provider.id)}
            className={cx(
              "inline-flex cursor-pointer items-center gap-1.5 rounded-sm font-medium text-ink underline-offset-[3px] hover:underline",
              focusRing,
            )}
          >
            {provider.id === "github" ? <GitHubMark /> : null}
            {provider.label}
          </button>
        </span>
      ))}
    </p>
  );
}
