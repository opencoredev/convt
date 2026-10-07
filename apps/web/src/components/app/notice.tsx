import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

// Notices at the bottom of the account pages. `usePlaceholderAction` is for
// buttons whose backend does not exist yet: they say so instead of pretending to
// succeed. Apple sign-in is hidden instead until it is configured (CNV-20).
// `useNotice` confirms real actions, such as a copied key.

type Notify = (action: string) => void;

const NoticeContext = createContext<{ placeholder: Notify; show: Notify }>({
  placeholder: () => {},
  show: () => {},
});

export function usePlaceholderAction() {
  return useContext(NoticeContext).placeholder;
}

/** A short confirmation or error at the bottom of the page, such as "Key copied". */
export function useNotice() {
  return useContext(NoticeContext).show;
}

export function NoticeProvider({ children }: { children: React.ReactNode }) {
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const show = useCallback<Notify>((text) => {
    clearTimeout(timer.current);
    setMessage(text);
    timer.current = setTimeout(() => setMessage(null), 4000);
  }, []);
  const placeholder = useCallback<Notify>(
    (action) => show(`${action} is not available yet. The account service is still being built.`),
    [show],
  );
  const value = useMemo(() => ({ placeholder, show }), [placeholder, show]);

  useEffect(() => () => clearTimeout(timer.current), []);

  return (
    <NoticeContext.Provider value={value}>
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
