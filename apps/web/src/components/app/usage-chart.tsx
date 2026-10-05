import { formatAxisDate, formatNumber, formatShortDate } from "#/lib/format";
import type { UsageDay } from "#/lib/types";

import { cx } from "./ui";

/** Bars per day. Days in the same month as the last day are drawn in green. */
export function UsageChart({ days }: { days: UsageDay[] }) {
  if (days.length === 0) {
    return <p className="text-[13px]/4 text-ink-3">No conversions in the last 30 days.</p>;
  }
  const max = Math.max(1, ...days.map((d) => d.conversions));
  const first = days[0].date;
  const last = days[days.length - 1].date;
  const middle = days[Math.floor((days.length - 1) / 2)].date;
  const currentMonth = last.slice(0, 7);
  const total = days.reduce((sum, d) => sum + d.conversions, 0);

  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex justify-between gap-4">
        <h2 className="text-[13px]/4 text-ink-2">Conversions per day</h2>
        <span className="font-mono text-[11px]/3.5 text-ink-3">
          {formatAxisDate(first)} – {formatAxisDate(last)}
        </span>
      </div>
      <div
        role="img"
        aria-label={`${formatNumber(total)} conversions from ${formatShortDate(first)} to ${formatShortDate(last)}, at most ${formatNumber(max)} in a day.`}
        className="flex h-[170px] shrink-0 items-end gap-0.5 border-b border-line sm:gap-1.5"
      >
        {days.map((day) => (
          <div
            key={day.date}
            title={`${formatShortDate(day.date)}: ${formatNumber(day.conversions)}`}
            style={{ height: `${Math.round((day.conversions / max) * 100)}%` }}
            className={cx(
              "min-w-0 flex-1 rounded-t-[3px]",
              day.date.startsWith(currentMonth) ? "bg-green" : "bg-bar",
            )}
          />
        ))}
      </div>
      <div aria-hidden="true" className="flex justify-between font-mono text-[11px]/3.5 text-ink-3">
        <span>{formatAxisDate(first)}</span>
        <span>{formatAxisDate(middle)}</span>
        <span>{formatAxisDate(last)}</span>
      </div>
    </div>
  );
}
