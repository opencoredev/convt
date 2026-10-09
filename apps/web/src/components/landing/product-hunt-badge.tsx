import { useEffect, useState } from "react";

import { PRODUCT_HUNT_FROM, PRODUCT_HUNT_URL } from "#/lib/site";

import { cx, focusRing } from "./ui";

/**
 * The Product Hunt "featured" badge. The prerendered HTML never contains it; the browser
 * shows it once PRODUCT_HUNT_FROM has passed, including on a page left open past that time.
 */
export function ProductHuntBadge({ className }: { className?: string }) {
  const [live, setLive] = useState(false);

  useEffect(() => {
    const wait = PRODUCT_HUNT_FROM - Date.now();
    if (wait <= 0) {
      setLive(true);
      return;
    }
    // setTimeout overflows past about 24.8 days; a page open that long can reload.
    if (wait > 2 ** 31 - 1) return;
    const timer = setTimeout(() => setLive(true), wait);
    return () => clearTimeout(timer);
  }, []);

  if (!live) return null;

  return (
    <a
      href={PRODUCT_HUNT_URL}
      target="_blank"
      rel="noopener noreferrer"
      className={cx("inline-flex rounded-[10px]", focusRing, className)}
    >
      {/* The badge follows the visitor's theme, as the page does. */}
      <picture>
        <source
          media="(prefers-color-scheme: dark)"
          srcSet="https://api.producthunt.com/widgets/embed-image/v1/featured.svg?post_id=1272021&theme=dark&t=1791317983505"
        />
        <img
          alt="convt - Convert any file with a right-click, without uploading it | Product Hunt"
          width={250}
          height={54}
          src="https://api.producthunt.com/widgets/embed-image/v1/featured.svg?post_id=1272021&theme=light&t=1791317983505"
        />
      </picture>
    </a>
  );
}
