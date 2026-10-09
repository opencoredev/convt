import { useEffect } from "react";

// The design has no theme picker, so every page follows the OS setting. The inline
// script runs in <head> before first paint so nobody sees the wrong theme flash; the
// hook keeps the class in sync afterwards, including when the OS setting changes and
// on client-side navigation. A route can pin a theme (`staticData.theme`, the brand
// page); the root renders it as `data-theme` on <html>, which both of them honor.
// React never owns the `dark` class, so a re-render can't strip it.

const query = "(prefers-color-scheme: dark)";

export type Theme = "light" | "dark";

export const themeScript = `(function(){try{var d=document.documentElement,p=d.getAttribute("data-theme");d.classList.toggle("dark",p?p==="dark":window.matchMedia(${JSON.stringify(query)}).matches)}catch(e){}})()`;

export function useSystemTheme(pinned: Theme | undefined) {
  useEffect(() => {
    const media = window.matchMedia(query);
    const apply = () =>
      document.documentElement.classList.toggle("dark", pinned ? pinned === "dark" : media.matches);
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [pinned]);
}
