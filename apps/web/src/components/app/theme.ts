import { useEffect } from "react";

// The design has no theme picker, so the pages follow the OS setting. The inline
// script runs in <head> before first paint so a dark-mode visitor never sees a white
// flash; the hook keeps the class in sync afterwards and covers client-side
// navigation from pages that did not run the script.

const query = "(prefers-color-scheme: dark)";

export const themeScript = `(function(){try{document.documentElement.classList.toggle("dark",window.matchMedia(${JSON.stringify(query)}).matches)}catch(e){}})()`;

export function useSystemTheme() {
  useEffect(() => {
    const media = window.matchMedia(query);
    const apply = () => document.documentElement.classList.toggle("dark", media.matches);
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, []);
}
