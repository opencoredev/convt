import { cx } from "./ui";

/** An inline error under a form, announced to screen readers. */
export function FormError({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  if (!children) return null;
  return (
    <p role="alert" className={cx("text-[13px]/[18px] text-error", className)}>
      {children}
    </p>
  );
}

/** An inline confirmation, such as "We sent a new code." */
export function FormStatus({ children }: { children: React.ReactNode }) {
  if (!children) return null;
  return (
    <p role="status" className="text-[13px]/[18px] text-ink-2">
      {children}
    </p>
  );
}
