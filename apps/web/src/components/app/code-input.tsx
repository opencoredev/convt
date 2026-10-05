import { useRef, useState } from "react";

/**
 * One box per digit. Typing moves forward, Backspace moves back, and pasting a whole
 * code fills every box. Calls `onComplete` once all boxes have a digit.
 */
export function CodeInput({
  length,
  labelId,
  onComplete,
}: {
  length: number;
  labelId: string;
  onComplete: (code: string) => void;
}) {
  const [digits, setDigits] = useState<string[]>(() => Array.from({ length }, () => ""));
  const refs = useRef<Array<HTMLInputElement | null>>([]);

  function update(next: string[], focusIndex: number) {
    setDigits(next);
    refs.current[Math.min(focusIndex, length - 1)]?.focus();
    if (next.every((d) => d !== "")) onComplete(next.join(""));
  }

  function fill(from: number, text: string) {
    const incoming = text
      .replace(/\D/g, "")
      .slice(0, length - from)
      .split("");
    if (incoming.length === 0) return;
    const next = [...digits];
    incoming.forEach((d, i) => {
      next[from + i] = d;
    });
    update(next, from + incoming.length);
  }

  return (
    <div role="group" aria-labelledby={labelId} className="flex gap-2">
      {digits.map((digit, i) => (
        <input
          key={i}
          ref={(el) => {
            refs.current[i] = el;
          }}
          value={digit}
          inputMode="numeric"
          autoComplete={i === 0 ? "one-time-code" : "off"}
          maxLength={length}
          aria-label={`Digit ${i + 1} of ${length}`}
          onChange={(event) => {
            const value = event.target.value;
            if (value === "") {
              const next = [...digits];
              next[i] = "";
              setDigits(next);
            } else {
              // A single character replaces the box, even when it repeats the old digit.
              // Typing into a filled box without a selection appends; keep the new part.
              const typed =
                value.length > 1 && digit && value.startsWith(digit)
                  ? value.slice(digit.length)
                  : value;
              fill(i, typed);
            }
          }}
          onKeyDown={(event) => {
            if (event.key === "Backspace" && digit === "" && i > 0) {
              event.preventDefault();
              const next = [...digits];
              next[i - 1] = "";
              setDigits(next);
              refs.current[i - 1]?.focus();
            } else if (event.key === "ArrowLeft" && i > 0) {
              refs.current[i - 1]?.focus();
            } else if (event.key === "ArrowRight" && i < length - 1) {
              refs.current[i + 1]?.focus();
            }
          }}
          onPaste={(event) => {
            event.preventDefault();
            fill(i, event.clipboardData.getData("text"));
          }}
          onFocus={(event) => event.currentTarget.select()}
          className="size-11 min-w-0 rounded-lg bg-raised text-center font-mono text-xl/6 text-ink shadow-input outline-none focus:shadow-[0_0_0_2px_var(--green)] sm:size-[52px]"
        />
      ))}
    </div>
  );
}
