import { useEffect, useState } from "react";

import { SecondaryButton } from "#/components/app/ui";
import { analyticsChoice, setAnalyticsOptOut, type AnalyticsChoice } from "#/lib/analytics-consent";

const status: Record<AnalyticsChoice, string> = {
  on: "Analytics are on in this browser.",
  off: "Analytics are off in this browser.",
  browser: "Analytics are off: your browser sends Do Not Track or Global Privacy Control.",
};

/** The privacy page's switch for PostHog in this browser. Renders after hydration. */
export function AnalyticsOptOut() {
  const [choice, setChoice] = useState<AnalyticsChoice | null>(null);
  useEffect(() => setChoice(analyticsChoice()), []);
  if (choice === null) return null;

  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <p role="status" className="text-ink">
        {status[choice]}
      </p>
      {choice !== "browser" && (
        <SecondaryButton
          onClick={() => {
            setAnalyticsOptOut(choice === "on");
            setChoice(analyticsChoice());
          }}
        >
          {choice === "on" ? "Turn off analytics" : "Turn analytics back on"}
        </SecondaryButton>
      )}
    </div>
  );
}
