import { cx, focusRing } from "#/components/app/ui";

// Sign-in providers. Buttons are text-only by Leo's decision. Each one renders only
// when the Worker has it configured; Apple waits for the Apple Developer account
// (CNV-20), so its button is hidden until then.
const providers = [
  { id: "github", label: "GitHub" },
  { id: "google", label: "Google" },
  { id: "apple", label: "Apple" },
] as const;

export type SocialProviderId = (typeof providers)[number]["id"];

/** "or continue with" and a button per configured provider; nothing when none is. */
export function SocialSignIn({
  available,
  onSelect,
}: {
  available: Record<SocialProviderId, boolean>;
  onSelect: (provider: SocialProviderId) => void;
}) {
  const shown = providers.filter((provider) => available[provider.id]);
  if (shown.length === 0) return null;
  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex items-center gap-3">
        <span className="h-px flex-1 bg-line" />
        <span className="text-xs/4 text-ink-3">or continue with</span>
        <span className="h-px flex-1 bg-line" />
      </div>
      <div className="flex gap-2">
        {shown.map((provider) => (
          <button
            key={provider.id}
            type="button"
            onClick={() => onSelect(provider.id)}
            className={cx(
              "h-10 flex-1 cursor-pointer rounded-lg bg-raised text-[13px]/4 font-medium shadow-button hover:bg-hover dark:bg-sunken",
              focusRing,
            )}
          >
            {provider.label}
          </button>
        ))}
      </div>
    </div>
  );
}
