import { ArrowRight01Icon, CheckmarkCircle02Icon } from "@hugeicons/core-free-icons";
import { useEffect, useRef } from "react";

import { Icon } from "#/components/icon";
import { Mark } from "#/components/logo";

import { MisoPhoto } from "./ui";

/*
 * The hero's explanation: a pointer right-clicks miso.heic, opens convt's submenu, picks
 * WebP, and miso.webp appears next to the original. Every part runs on one 10-second CSS
 * timeline (`demo-*` keyframes in styles.css), so it stays smooth while the page loads
 * and costs nothing off screen: the loop pauses outside the viewport. With reduced
 * motion the stage shows the finished state: both files, no menu, no pointer.
 *
 * The stage is laid out in em on a 40em by 24em board whose font size follows the
 * container width, so it scales down on phones without a second layout and never
 * shifts what is around it.
 */

const menu = ["Open", "Get Info", "Rename", "Compress"];
const formats = ["WebP", "JPEG", "PNG", "AVIF", "PDF"];

export function ConvertDemo() {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const stage = ref.current;
    if (!stage) return;
    const observer = new IntersectionObserver(([entry]) => {
      stage.dataset.playing = entry.isIntersecting ? "true" : "false";
    });
    observer.observe(stage);
    return () => observer.disconnect();
  }, []);

  return (
    <div className="demo-figure">
      <figure className="bg-land-glow relative overflow-clip rounded-2xl bg-bottom shadow-[inset_0_0_0_1px_var(--line)] dark:shadow-none px-4 pt-10 pb-8 sm:px-8 lg:pt-20 lg:pb-12">
        <figcaption className="sr-only">
          Right-click miso.heic, choose Convert with convt, then WebP: miso.webp, 612 KB, appears
          next to the original.
        </figcaption>
        <div className="@container mx-auto w-full max-w-[880px]">
          <div
            ref={ref}
            aria-hidden="true"
            data-playing="false"
            className="demo relative mx-auto select-none"
          >
            {/* The source file, selected while the menu is open. */}
            <div className="absolute top-(--src-y) left-(--src-x) flex w-[9em] flex-col items-center gap-[0.5em]">
              <div className="demo-select rounded-[0.7em] p-[0.3em]">
                <MisoPhoto
                  alt=""
                  width={300}
                  height={220}
                  className="aspect-[300/220] w-[8.4em] rounded-[0.5em] object-cover shadow-land-float"
                />
              </div>
              <span className="demo-select-label rounded-[0.35em] px-[0.4em] text-[0.62em]/[1.5] font-medium text-ink">
                miso.heic
              </span>
              <span className="-mt-[0.4em] font-mono text-[0.52em]/[1.4] text-land-muted">
                4.8 MB
              </span>
            </div>

            {/* The context menu, then convt's submenu. */}
            <div className="demo-menu absolute top-(--menu-y) left-(--menu-x)">
              <div className="w-[14.5em] rounded-[0.6em] whitespace-nowrap bg-land-menu p-[0.3em] text-[0.62em]/[1.6] text-ink shadow-land-menu backdrop-blur-md">
                {menu.map((item) => (
                  <div
                    key={item}
                    className="rounded-[0.35em] px-[0.7em] py-[0.15em] text-land-menu-ink"
                  >
                    {item}
                  </div>
                ))}
                <div className="mx-[0.7em] my-[0.3em] h-px bg-land-menu-line" />
                <div className="demo-convt flex items-center gap-[0.5em] rounded-[0.35em] px-[0.7em] py-[0.15em]">
                  <Mark size={14} className="size-[1.1em]" />
                  <span className="flex-1">Convert with convt</span>
                  <Icon icon={ArrowRight01Icon} size={12} className="size-[1em]" />
                </div>
              </div>
            </div>

            <div className="demo-sub absolute top-(--sub-y) left-(--sub-x)">
              <div className="w-[7em] rounded-[0.6em] bg-land-menu p-[0.3em] text-[0.62em]/[1.6] text-land-menu-ink shadow-land-menu backdrop-blur-md">
                {formats.map((format, i) => (
                  <div
                    key={format}
                    className={
                      i === 0
                        ? "demo-pick rounded-[0.35em] px-[0.7em] py-[0.15em]"
                        : "rounded-[0.35em] px-[0.7em] py-[0.15em]"
                    }
                  >
                    {format}
                  </div>
                ))}
              </div>
            </div>

            {/* The result lands next to the original. */}
            <div className="demo-result absolute top-(--res-y) left-(--res-x) flex w-[9em] flex-col items-center gap-[0.5em]">
              <div className="relative p-[0.3em]">
                <MisoPhoto
                  alt=""
                  width={300}
                  height={220}
                  className="aspect-[300/220] w-[8.4em] rounded-[0.5em] object-cover shadow-land-float"
                />
                <span className="demo-badge absolute -top-[0.2em] -right-[0.2em] flex size-[1.5em] items-center justify-center rounded-full bg-[#22a867] text-white shadow-[0_0_0_0.15em_var(--page)]">
                  <Icon
                    icon={CheckmarkCircle02Icon}
                    size={16}
                    strokeWidth={2}
                    className="size-[1.1em]"
                  />
                </span>
              </div>
              <span className="text-[0.62em]/[1.5] font-medium text-ink">miso.webp</span>
              <span className="relative -mt-[0.4em] h-[1.4em] w-[6em] font-mono text-[0.52em]/[1.4]">
                <span className="demo-bar absolute inset-x-0 top-1/2 h-[0.3em] -translate-y-1/2 overflow-clip rounded-full bg-line-strong">
                  <span className="demo-bar-fill block h-full w-full origin-left rounded-full bg-land-accent" />
                </span>
                <span className="demo-size absolute inset-0 text-center text-green">612 KB</span>
              </span>
            </div>

            {/* What the conversion saved, between the two files. */}
            <div className="demo-arrow absolute top-[8.4em] left-[12.4em] flex w-[15em] items-center gap-[0.5em]">
              <span className="h-px flex-1 bg-[linear-gradient(90deg,transparent,#4cc28399)]" />
              <span className="rounded-full bg-land-pill px-[0.7em] py-[0.2em] font-mono text-[0.52em]/[1.5] text-land-accent shadow-[0_0_0_1px_#4cc28359]">
                HEIC → WebP · 87% smaller
              </span>
              <span className="h-px flex-1 bg-[linear-gradient(90deg,#4cc28399,transparent)]" />
            </div>

            {/* The pointer. */}
            <svg
              viewBox="0 0 24 24"
              className="demo-cursor absolute top-0 left-0 size-[1.3em] drop-shadow-[0_0.1em_0.25em_#000000aa]"
            >
              <path
                d="M5 3.2v16.4l4.3-4.1 2.7 6.1 2.9-1.3-2.7-6h6z"
                fill="#ffffff"
                stroke="#0a0b0b"
                strokeWidth="1.2"
                strokeLinejoin="round"
              />
            </svg>
          </div>
        </div>
      </figure>
      <ol
        aria-hidden="true"
        className="demo-steps mt-5 flex flex-wrap justify-center gap-x-6 gap-y-2 font-mono text-[12px]/[16px] text-land-muted"
      >
        <li className="demo-step demo-step-1">1 · Right-click</li>
        <li className="demo-step demo-step-2">2 · Pick a format</li>
        <li className="demo-step demo-step-3">3 · Done, next to the original</li>
      </ol>
    </div>
  );
}
