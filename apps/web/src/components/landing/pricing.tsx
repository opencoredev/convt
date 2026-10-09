import { useState } from "react";

import { BUY_DESKTOP_URL, LAUNCHED, buyProUrl, routes } from "#/lib/site";

import { ButtonLink, ComingSoon, Container, cx, focusRing } from "./ui";

export function Pricing({
  sales,
  launched = LAUNCHED,
}: {
  sales: "desktop" | "all";
  launched?: boolean;
}) {
  return (
    <Container
      id="pricing"
      className="flex scroll-mt-6 flex-col items-center gap-10 pt-16 pb-16 md:pt-[140px] md:pb-[120px]"
    >
      <div className="flex flex-col items-center gap-3.5 text-center">
        <h2 className="text-[34px]/[40px] font-medium tracking-[-0.035em] text-ink md:text-[44px]/[48px]">
          Buy it once, or go Pro.
        </h2>
        <p className="text-[17px]/[26px] text-ink-2">convt Pro starts with a 7-day free trial.</p>
      </div>
      <div className="grid w-full max-w-[840px] gap-4 md:grid-cols-2">
        <DesktopPlan launched={launched} />
        <ProPlan launched={launched} />
      </div>
      <ApiCard available={sales === "all"} launched={launched} />
    </Container>
  );
}

function PlanHeader({ name, description }: { name: string; description: string }) {
  return (
    <>
      <h3 className="text-[16px]/[20px] font-medium text-ink">{name}</h3>
      <p className="text-[14px]/[21px] text-ink-2">{description}</p>
    </>
  );
}

function Price({ amount, unit, note }: { amount: string; unit: string; note: string }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
      <span className="text-[44px]/[54px] font-medium tracking-[-0.04em] text-ink">{amount}</span>
      <span className="text-[14px]/[18px] text-ink-2">{unit}</span>
      <span className="w-full text-[13px]/[16px] text-ink-2">{note}</span>
    </div>
  );
}

// The design spaces the check mark by one space on Desktop and two on Pro.
function Features({ items, mark = "✓ " }: { items: string[]; mark?: string }) {
  return (
    <ul className="flex flex-col gap-2.5">
      {items.map((item) => (
        <li key={item} className="text-[14px]/[18px] whitespace-pre-wrap text-ink">
          <span aria-hidden="true">{mark}</span>
          {item}
        </li>
      ))}
    </ul>
  );
}

const planCard = "flex flex-col gap-6 rounded-2xl bg-raised p-7";

function DesktopPlan({ launched }: { launched: boolean }) {
  return (
    <div
      className={cx(
        planCard,
        "shadow-[0_0_0_1px_var(--line),0_1px_2px_#0000000a,0_8px_24px_#0a1e140f] dark:shadow-[0_0_0_1px_#232726,0_1px_2px_#00000066,0_8px_24px_#00000066]",
      )}
    >
      <div className="flex flex-col gap-2">
        <PlanHeader
          name="Desktop"
          description="The app and the right-click menu, on your machine."
        />
      </div>
      <Price
        amount="$29"
        unit="once"
        note="Includes 12 months of updates. Keep your version forever."
      />
      <Features
        items={[
          "Every format, offline",
          "macOS, Windows and Linux",
          "12 months of updates",
          "Batch folders and presets",
        ]}
      />
      {launched ? (
        <ButtonLink
          variant="secondary"
          href={BUY_DESKTOP_URL}
          className="mt-auto h-10 w-full rounded-[10px] text-[14px]/[18px]"
        >
          Get Desktop License
        </ButtonLink>
      ) : (
        <ComingSoon className="mt-auto h-10 w-full rounded-[10px] text-[14px]/[18px]" />
      )}
    </div>
  );
}

function ProPlan({ launched }: { launched: boolean }) {
  const [yearly, setYearly] = useState(false);
  return (
    <div
      className={cx(
        planCard,
        "shadow-[0_0_0_1px_#1fa463,0_8px_30px_#0a3c231f] dark:shadow-[0_0_0_1px_#1fa463,0_8px_30px_#00000080]",
      )}
    >
      <div className="relative flex flex-col gap-2">
        <PlanHeader
          name="Pro"
          description="Everything in Desktop, plus cloud conversions when you choose them."
        />
        <div
          role="group"
          aria-label="Billing period"
          className="absolute -top-0.5 right-0 flex items-center gap-0.5 rounded-lg bg-sunken p-0.5"
        >
          <PeriodButton active={!yearly} onClick={() => setYearly(false)}>
            Monthly
          </PeriodButton>
          <PeriodButton active={yearly} onClick={() => setYearly(true)}>
            Yearly
          </PeriodButton>
        </div>
      </div>
      {yearly ? (
        <Price amount="$8" unit="per month" note="Billed yearly. Save 33% over monthly." />
      ) : (
        <Price amount="$12" unit="per month" note="$8 a month if you pay yearly. Save 33%." />
      )}
      <Features
        mark="✓  "
        items={[
          "Everything in Desktop",
          "Convert from phone or browser",
          "Send heavy video jobs to the cloud",
          "Every future update included",
        ]}
      />
      {launched ? (
        <ButtonLink
          variant="primary"
          href={buyProUrl(yearly ? "year" : "month")}
          className="mt-auto h-10 w-full rounded-[10px] text-[14px]/[18px]"
        >
          Get convt Pro
        </ButtonLink>
      ) : (
        <ComingSoon className="mt-auto h-10 w-full rounded-[10px] text-[14px]/[18px]">
          Pro is not on sale yet
        </ComingSoon>
      )}
    </div>
  );
}

function PeriodButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cx(
        "flex h-6 cursor-pointer items-center rounded-md px-2.5 text-[12px]/[16px] font-medium transition-colors",
        active
          ? "bg-raised text-ink shadow-[0_0_0_1px_var(--line-strong),0_1px_1px_#0000000f] dark:bg-hover dark:shadow-[inset_0_1px_0_#ffffff0f,0_0_0_1px_#2e3331,0_1px_1px_#00000066]"
          : "text-land-muted hover:text-ink-2",
        focusRing,
      )}
    >
      {children}
    </button>
  );
}

type Token = [color: string, text: string];

const kw = "text-[#7fd3a6]";
const id = "text-land-mono";
const punct = "text-land-muted";
const str = "text-[#f2c46d]";
const fn = "text-[#9cc9ff]";

const code: Token[][] = [
  [
    [kw, "import"],
    [punct, " { "],
    [id, "Convt"],
    [punct, " } "],
    [kw, "from"],
    [id, " "],
    [str, '"@convt/sdk"'],
    [punct, ";"],
  ],
  [[id, " "]],
  [
    [kw, "const"],
    [id, " convt "],
    [punct, "= "],
    [kw, "new"],
    [id, " "],
    [fn, "Convt"],
    [punct, "();"],
  ],
  [
    [kw, "const"],
    [id, " out "],
    [punct, "= "],
    [kw, "await"],
    [id, " convt"],
    [punct, "."],
    [fn, "convert"],
    [punct, "("],
    [str, '"report.docx"'],
    [punct, ", {"],
  ],
  [
    [id, "  to"],
    [punct, ": "],
    [str, '"pdf"'],
    [punct, ","],
  ],
  [[punct, "});"]],
  [
    [kw, "await"],
    [id, " out"],
    [punct, "."],
    [fn, "save"],
    [punct, "("],
    [str, '"report.pdf"'],
    [punct, ");"],
  ],
];

function ApiCard({ available, launched }: { available: boolean; launched: boolean }) {
  return (
    <div
      id="api"
      className="flex w-full max-w-[840px] scroll-mt-6 flex-col gap-8 rounded-2xl bg-raised p-6 shadow-[0_0_0_1px_var(--line),0_8px_24px_#0a1e140f] lg:flex-row dark:shadow-[0_0_0_1px_#232726,0_8px_24px_#00000066] lg:items-center lg:justify-between lg:py-7 lg:pr-7 lg:pl-8"
    >
      <div className="flex flex-col gap-4 lg:w-[300px] lg:shrink-0">
        <div className="flex flex-col gap-2">
          <h3 className="text-[22px]/[28px] font-medium tracking-[-0.02em] text-ink">
            Convert files from your own code.
          </h3>
          <p className="text-[14px]/[21px] text-ink-2">
            Same engines as the app. Pay per conversion, no plan needed.
          </p>
        </div>
        <div className="flex">
          {launched && available ? (
            <ButtonLink
              variant="secondary"
              href={routes.apiDocs}
              className="h-9 rounded-[9px] px-3.5 text-[14px]/[18px]"
            >
              Read the docs
            </ButtonLink>
          ) : (
            <ComingSoon className="h-9 rounded-[9px] px-3.5 text-[14px]/[18px]">
              API is not on sale yet
            </ComingSoon>
          )}
        </div>
      </div>
      {/* The code sample stays dark in both themes, like a terminal. */}
      <div className="dark flex min-w-0 flex-col rounded-[10px] bg-land-code text-ink shadow-[inset_0_0_0_1px_#232726] lg:w-[440px] lg:shrink-0">
        <div className="flex h-[34px] shrink-0 items-center px-3.5 font-mono text-[11px]/[14px] text-land-muted shadow-[inset_0_-1px_0_#ffffff0f]">
          convert.ts
        </div>
        <pre className="overflow-x-auto pt-4 pr-4 pb-[18px] pl-3.5 font-mono text-[12.5px]/[21px]">
          <code>
            {code.map((line, i) => (
              <span key={i} className="flex">
                <span aria-hidden="true" className="w-7 shrink-0 text-[#3e4441] select-none">
                  {i + 1}
                </span>
                {line.map(([color, text], j) => (
                  <span key={j} className={color}>
                    {text}
                  </span>
                ))}
              </span>
            ))}
          </code>
        </pre>
      </div>
    </div>
  );
}
