import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";

/**
 * A Hugeicons (free, MIT) stroke icon, decorative by default. Brand marks (the convt
 * logo, Google, GitHub) are not icons and stay as their own SVGs.
 */
export function Icon({
  icon,
  size = 16,
  strokeWidth = 1.6,
  className,
}: {
  icon: IconSvgElement;
  size?: number;
  strokeWidth?: number;
  className?: string;
}) {
  return (
    <HugeiconsIcon
      icon={icon}
      size={size}
      strokeWidth={strokeWidth}
      aria-hidden="true"
      className={className ? `shrink-0 ${className}` : "shrink-0"}
    />
  );
}
