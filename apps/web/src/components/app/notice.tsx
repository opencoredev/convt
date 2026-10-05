import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";

// PLACEHOLDER FLOW. Buttons that would call the account API (revoke a key, cancel a
// plan, sign out a Mac, ...) show this notice instead of pretending to succeed.
// Remove it once the real actions exist.

type Notify = (action: string) => void;

const NoticeContext = createContext<Notify>(() => {});

export function usePlaceholderAction() {
  return useContext(NoticeContext);
}

export function NoticeProvider({ children }: { children: React.ReactNode }) {
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const notify = useCallback<Notify>((action) => {
    clearTimeout(timer.current);
    setMessage(`${action} is not available yet. The account service is still being built.`);
    timer.current = setTimeout(() => setMessage(null), 4000);
  }, []);

  useEffect(() => () => clearTimeout(timer.current), []);

  return (
    <NoticeContext.Provider value={notify}>
      {children}
      <div
        role="status"
        aria-live="polite"
        className="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex justify-center px-5"
      >
        {message ? (
          <div className="max-w-md rounded-lg bg-ink px-4 py-2.5 text-[13px]/4 text-page shadow-lg">
            {message}
          </div>
        ) : null}
      </div>
    </NoticeContext.Provider>
  );
}
