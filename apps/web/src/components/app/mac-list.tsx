import { useRouter } from "@tanstack/react-router";

import type { Mac } from "#/lib/types";
import { endSession } from "#/server/account-fns";

import { useNotice } from "./notice";
import { SectionTitle, TextButton } from "./ui";

/** The "Macs" list on the overview and licenses pages. */
export function MacList({ macs }: { macs: Mac[] }) {
  const notice = useNotice();
  const router = useRouter();

  return (
    <section aria-labelledby="macs-title" className="flex flex-col pt-4">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 pb-3">
        <SectionTitle id="macs-title">Macs</SectionTitle>
        <p className="text-[13px]/4 text-ink-2">Sign out a Mac to move your license.</p>
      </div>
      {macs.length === 0 ? (
        <p className="border-y border-line py-3.5 text-sm/4.5 text-ink-2">
          No Macs yet. Sign in inside the app to add one.
        </p>
      ) : (
        <ul className="border-b border-line">
          {macs.map((mac) => (
            <li
              key={mac.id}
              className="grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 border-t border-line py-3.5 sm:flex"
            >
              <span className="text-sm/4.5 sm:flex-1">{mac.name}</span>
              <span className="col-start-1 row-start-2 font-mono text-xs/4 text-ink-2 sm:w-[200px] sm:shrink-0">
                {mac.os}
                <span className="sm:hidden"> · Seen {mac.lastSeen}</span>
              </span>
              <span className="hidden font-mono text-xs/4 text-ink-2 sm:block sm:w-[200px] sm:shrink-0">
                Seen {mac.lastSeen}
              </span>
              <span className="row-span-2 text-right sm:w-20 sm:shrink-0">
                <TextButton
                  tone="ink"
                  onClick={async () => {
                    try {
                      await endSession({ data: { id: mac.id, type: "device" } });
                      notice(`Signed out ${mac.name}.`);
                      await router.invalidate();
                    } catch {
                      notice(`Couldn't sign out ${mac.name}. Try again.`);
                    }
                  }}
                  aria-label={`Sign out ${mac.name}`}
                >
                  Sign out
                </TextButton>
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
