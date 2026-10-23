import { useEffect, useRef, useState } from "react";

import { cx } from "./ui";

// The interaction follows the RareUI OTP input (rareui.com/components/otpinput):
// digits roll into place, one caret glides between boxes, a wrong code shakes and
// clears, a right one draws a ring around each box. Written for convt in CSS; the
// keyframes are the `code-*` rules in styles.css.

/**
 * Where the input is in its life. `checking` waits for `verify`; `rejected` plays the
 * shake before the boxes clear, and typing cuts it short; `error` keeps the red ring
 * until the next edit.
 */
type Phase = "idle" | "checking" | "rejected" | "error" | "accepted";

// Shake (320 ms) and the digits dropping out (from 240 ms, 180 ms long) finish here.
const REJECT_MS = 440;
// The success ring around the last box (45 ms stagger, 450 ms draw) finishes here.
const ACCEPT_MS = 700;

/**
 * Puts `text`'s digits into `digits` from box `at`, or from the first box when it holds
 * a whole code (a paste or an SMS autofill landing in any box). Returns the new boxes
 * and the box to focus next, or null when `text` has no digits.
 */
export function placeDigits(
  digits: readonly string[],
  at: number,
  text: string,
): { digits: string[]; focus: number } | null {
  const incoming = text.replace(/\D/g, "").split("");
  if (incoming.length === 0) return null;
  const start = incoming.length >= digits.length ? 0 : at;
  const placed = incoming.slice(0, digits.length - start);
  const next = [...digits];
  placed.forEach((digit, k) => {
    next[start + k] = digit;
  });
  return { digits: next, focus: Math.min(start + placed.length, digits.length - 1) };
}

/**
 * What the user typed into a box that held `previous`. A box with its digit selected
 * gets replaced, but one without a selection gets the new character before or after
 * the old one; keep only the new part.
 */
export function typedText(previous: string, value: string): string {
  if (previous && value.length === previous.length + 1) {
    if (value.startsWith(previous)) return value.slice(previous.length);
    if (value.endsWith(previous)) return value.slice(0, 1);
  }
  return value;
}

/**
 * One box per digit. Typing moves forward, Backspace moves back across boxes, and a
 * pasted or autofilled code fills every box. The moment the last box has a digit the
 * code goes to `verify`, once: no Enter and no button. `verify` resolves true when the
 * code was accepted; on false the boxes shake, clear, and focus the first box again.
 * `onAccepted` runs once the success ring has drawn, for a caller that removes the
 * input rather than leaving the page. The group fills its column, so the boxes can
 * shrink on a narrow screen; `align` places them in it.
 */
export function CodeInput({
  length,
  labelId,
  verify,
  onAccepted,
  autoFocus = false,
  align = "start",
}: {
  length: number;
  labelId: string;
  verify: (code: string) => Promise<boolean>;
  onAccepted?: () => void;
  autoFocus?: boolean;
  align?: "start" | "center";
}) {
  const [digits, setDigits] = useState<string[]>(() => Array.from({ length }, () => ""));
  const [phase, setPhase] = useState<Phase>("idle");
  const [focused, setFocused] = useState<number | null>(null);
  // Bumped when the boxes clear, so a digit typed again after a wrong code rolls in.
  const [round, setRound] = useState(0);
  const inputs = useRef<Array<HTMLInputElement | null>>([]);
  // State lags a render behind; this blocks a second submit from the same tick.
  const submitting = useRef(false);
  const rejectTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const acceptTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // A reply that lands after the input is gone (Cancel mid-request) must not run
  // onAccepted, which could close a form opened since.
  const mounted = useRef(false);
  const locked = phase === "checking" || phase === "accepted";

  useEffect(() => {
    mounted.current = true;
    if (autoFocus) inputs.current[0]?.focus();
    return () => {
      mounted.current = false;
      clearTimeout(rejectTimer.current);
      clearTimeout(acceptTimer.current);
    };
  }, [autoFocus]);

  function focusAt(index: number) {
    const input = inputs.current[Math.min(Math.max(index, 0), length - 1)];
    input?.focus();
    input?.select();
  }

  async function submit(code: string) {
    if (submitting.current) return;
    submitting.current = true;
    setPhase("checking");
    let accepted = false;
    try {
      accepted = await verify(code);
    } catch {
      accepted = false;
    }
    if (!mounted.current) return;
    if (accepted) {
      setPhase("accepted");
      if (onAccepted) {
        const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        acceptTimer.current = setTimeout(onAccepted, still ? 0 : ACCEPT_MS);
      }
      return;
    }
    setPhase("rejected");
    rejectTimer.current = setTimeout(() => {
      reset();
      focusAt(0);
    }, REJECT_MS);
  }

  /** Clears the boxes after a wrong code. Returns the empty boxes. */
  function reset() {
    clearTimeout(rejectTimer.current);
    const cleared = Array.from({ length }, () => "");
    setDigits(cleared);
    setRound((n) => n + 1);
    setPhase("error");
    submitting.current = false;
    return cleared;
  }

  /** The boxes an edit starts from: empty, from the first box, while a wrong code clears. */
  function current(index: number) {
    return phase === "rejected" ? { from: reset(), at: 0 } : { from: digits, at: index };
  }

  function edit(next: string[], focus: number) {
    setDigits(next);
    if (phase === "error" || phase === "rejected") setPhase("idle");
    focusAt(focus);
    if (next.every((digit) => digit !== "")) void submit(next.join(""));
  }

  function enter(index: number, text: string) {
    if (locked || !/\d/.test(text)) return;
    const { from, at } = current(index);
    const placed = placeDigits(from, at, text);
    if (placed) edit(placed.digits, placed.focus);
  }

  function clear(index: number, focus: number) {
    if (phase === "rejected") {
      reset();
      focusAt(0);
      return;
    }
    edit(
      digits.map((digit, i) => (i === index ? "" : digit)),
      focus,
    );
  }

  function onKeyDown(index: number, event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const { key } = event;
    // Handled here rather than in onChange so typing the digit a box already holds
    // still moves on: React fires no change when the value stays the same.
    if (/^\d$/.test(key)) {
      event.preventDefault();
      enter(index, key);
      return;
    }
    const moves: Record<string, number> = {
      ArrowLeft: index - 1,
      ArrowRight: index + 1,
      Home: 0,
      End: length - 1,
    };
    const move = moves[key];
    if (move !== undefined) {
      event.preventDefault();
      focusAt(move);
      return;
    }
    if (locked) {
      if (key === "Backspace" || key === "Delete") event.preventDefault();
      return;
    }
    if (key === "Backspace") {
      event.preventDefault();
      if (digits[index]) clear(index, index);
      else if (index > 0) clear(index - 1, index - 1);
    } else if (key === "Delete") {
      event.preventDefault();
      clear(index, index);
    }
  }

  // A click past the first empty box lands on it, so the code stays in one piece.
  function onPointerDown(index: number, event: React.PointerEvent<HTMLInputElement>) {
    const firstEmpty = digits.indexOf("");
    if (firstEmpty === -1 || firstEmpty >= index) return;
    event.preventDefault();
    focusAt(firstEmpty);
  }

  const caretAt = focused !== null && !locked && digits[focused] === "" ? focused : null;

  return (
    <div
      role="group"
      aria-labelledby={labelId}
      aria-busy={phase === "checking"}
      className={cx(
        "flex w-full [--gap:8px] max-sm:@container max-sm:[--cell:min(44px,calc((100cqw_-_5*var(--gap))/6))] sm:[--cell:52px]",
        align === "center" && "justify-center",
      )}
    >
      <div
        data-phase={phase}
        className="code-row relative flex gap-(--gap)"
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget)) setFocused(null);
        }}
      >
        {digits.map((digit, i) => (
          <div key={i} className="relative size-(--cell)">
            <input
              ref={(el) => {
                inputs.current[i] = el;
              }}
              value={digit}
              readOnly={locked}
              inputMode="numeric"
              autoComplete={i === 0 ? "one-time-code" : "off"}
              maxLength={length}
              aria-label={`Digit ${i + 1} of ${length}`}
              aria-invalid={phase === "rejected" || phase === "error" ? true : undefined}
              onChange={(event) => {
                const value = event.target.value;
                if (value === "") {
                  if (!locked) clear(i, i);
                } else {
                  enter(i, typedText(digit, value));
                }
              }}
              onKeyDown={(event) => onKeyDown(i, event)}
              onPaste={(event) => {
                event.preventDefault();
                enter(i, event.clipboardData.getData("text"));
              }}
              onPointerDown={(event) => onPointerDown(i, event)}
              onFocus={(event) => {
                setFocused(i);
                event.currentTarget.select();
              }}
              className={cx(
                "size-full rounded-lg bg-raised text-center font-mono text-xl text-transparent caret-transparent shadow-input outline-none selection:bg-transparent",
                "transition-shadow duration-150",
                phase === "rejected" || phase === "error"
                  ? "shadow-[0_0_0_1.5px_var(--error)] focus:shadow-[0_0_0_2px_var(--error)]"
                  : phase === "accepted"
                    ? "shadow-[0_0_0_1px_var(--green-line)]"
                    : "focus:shadow-[0_0_0_2px_var(--green)]",
              )}
            />
            <span
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 grid place-items-center overflow-hidden"
            >
              {digit ? (
                <span
                  key={`${round}:${digit}`}
                  style={phase === "checking" ? { animationDelay: `${150 + i * 70}ms` } : undefined}
                  className="code-digit font-mono text-xl/none font-medium text-ink"
                >
                  {digit}
                </span>
              ) : null}
            </span>
            {phase === "accepted" ? (
              <svg
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 size-full overflow-visible"
              >
                <rect
                  width="100%"
                  height="100%"
                  rx="8"
                  pathLength={1}
                  style={{ animationDelay: `${i * 45}ms` }}
                  className="code-ring fill-none stroke-green stroke-2"
                />
              </svg>
            ) : null}
          </div>
        ))}
        {caretAt !== null ? (
          <span
            aria-hidden="true"
            style={{
              transform: `translate(calc(${caretAt} * (var(--cell) + var(--gap)) + var(--cell) / 2 - 1px), -50%)`,
            }}
            className="code-caret pointer-events-none absolute top-1/2 left-0 h-6 w-0.5 rounded-full bg-ink"
          />
        ) : null}
      </div>
      <span role="status" className="sr-only">
        {phase === "checking" ? "Checking the code" : phase === "accepted" ? "Code accepted" : ""}
      </span>
    </div>
  );
}
