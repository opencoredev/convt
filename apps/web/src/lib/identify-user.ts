// Links pageviews to the signed-in account. Only the user id is sent; never an
// email. Callers still have to respect analyticsChoice() (GPC, DNT, opt-out).

export type IdentifyClient = {
  identify: (distinctId: string) => void;
  reset: () => void;
};

export type IdentifyChoice = "on" | "off" | "browser";

/**
 * Identifies once per signed-in user and resets on sign-out or opt-out.
 * Returns the distinct id now associated with this browser, or null.
 */
export function syncIdentifiedUser(
  client: IdentifyClient,
  userId: string | null,
  choice: IdentifyChoice,
  previouslyIdentified: string | null,
): string | null {
  if (choice !== "on") {
    if (previouslyIdentified) client.reset();
    return null;
  }
  if (userId) {
    if (userId !== previouslyIdentified) client.identify(userId);
    return userId;
  }
  if (previouslyIdentified) client.reset();
  return null;
}
