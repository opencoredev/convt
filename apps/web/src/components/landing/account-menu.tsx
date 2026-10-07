import { useEffect, useId, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";

import { signOut } from "#/lib/auth-client";
import { links } from "#/lib/config";
import type { Account } from "#/lib/types";

import { cx, focusRing } from "./ui";

const itemClass = cx(
  "flex w-full cursor-pointer items-center rounded-md px-2.5 py-2 text-left text-[14px]/[18px] text-ink transition-colors hover:bg-hover",
  focusRing,
  "focus-visible:outline-offset-0",
);

/**
 * The signed-in control in the site nav: the avatar opens a small panel with the
 * account, its pages, and sign out. A disclosure rather than an ARIA menu, so Tab
 * moves through the links like any other list.
 */
export function AccountMenu({ account }: { account: Account }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panelId = useId();

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      button.current?.focus();
    };
    const onFocus = (event: FocusEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    document.addEventListener("focusin", onFocus);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("focusin", onFocus);
    };
  }, [open]);

  const close = () => setOpen(false);

  return (
    <div ref={root} className="relative">
      <button
        ref={button}
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((value) => !value)}
        className={cx("flex cursor-pointer rounded-full", focusRing)}
      >
        <span className="sr-only">Account: {account.name}</span>
        <Avatar account={account} />
      </button>
      <div
        id={panelId}
        hidden={!open}
        className="absolute top-[calc(100%+8px)] right-0 z-50 w-[240px] origin-top-right rounded-xl bg-raised p-1.5 shadow-note motion-safe:animate-[menu-in_140ms_cubic-bezier(0.23,1,0.32,1)]"
      >
        <div className="flex flex-col gap-0.5 px-2.5 pt-2 pb-2.5">
          <p className="truncate text-[14px]/[18px] font-medium text-ink">{account.name}</p>
          <p className="truncate text-[13px]/4 text-ink-2">{account.email}</p>
        </div>
        <div className="h-px bg-line" />
        <ul className="flex flex-col py-1">
          <li>
            <Link to="/dashboard" onClick={close} className={itemClass}>
              Dashboard
            </Link>
          </li>
          <li>
            <Link to="/account" onClick={close} className={itemClass}>
              Settings
            </Link>
          </li>
          <li>
            <a href={links.docs} onClick={close} className={itemClass}>
              Docs
            </a>
          </li>
          <li>
            <a href={links.help} onClick={close} className={itemClass}>
              Help
            </a>
          </li>
        </ul>
        <div className="h-px bg-line" />
        <div className="pt-1">
          <button
            type="button"
            onClick={async () => {
              await signOut();
              window.location.assign("/sign-in");
            }}
            className={itemClass}
          >
            Sign out
          </button>
        </div>
      </div>
    </div>
  );
}

function Avatar({ account }: { account: Account }) {
  const style = { boxShadow: "var(--avatar-ring) 0 0 0 1px" };
  if (account.avatarUrl) {
    return (
      <img
        src={account.avatarUrl}
        alt=""
        width={32}
        height={32}
        style={style}
        className="size-8 shrink-0 rounded-full object-cover"
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      style={style}
      className="flex size-8 shrink-0 items-center justify-center rounded-full bg-sunken text-[13px] font-medium text-ink-2"
    >
      {account.name.slice(0, 1).toUpperCase()}
    </span>
  );
}
