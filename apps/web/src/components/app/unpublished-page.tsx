import { Link } from "@tanstack/react-router";

import { cx, focusRing } from "./ui";

/** Stand-in for a page that is linked from the site but not written yet. */
export function UnpublishedPage({ title }: { title: string }) {
  return (
    <div className="flex min-h-screen flex-col bg-page px-6 py-10 text-ink sm:px-16">
      <Link
        to="/"
        className={cx(
          "self-start rounded-sm text-lg/5.5 font-semibold tracking-[-0.02em]",
          focusRing,
        )}
      >
        convt
      </Link>
      <main className="flex max-w-[380px] flex-1 flex-col justify-center gap-6">
        <div className="flex flex-col gap-2">
          <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">{title}</h1>
          <p className="text-sm/[22px] text-ink-2">This page isn't published yet.</p>
        </div>
        <Link
          to="/"
          className={cx(
            "self-start rounded-sm text-[13px]/4 font-medium text-green hover:underline hover:underline-offset-2",
            focusRing,
          )}
        >
          Back to convt.app
        </Link>
      </main>
    </div>
  );
}
