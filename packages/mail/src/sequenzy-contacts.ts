// Sequenzy's subscriber API, for campaign email. Contacts are keyed by convt's
// user id (Sequenzy's `externalId`), so an email change is an update, not a new
// contact. Only convt-billing uses this; it needs a key with `subscribers:write`,
// which the transactional key does not have.

export type ContactAttributes = {
  /** convt.app's signed preferences link; templates use it as {{preferencesUrl}}. */
  preferencesUrl: string;
  desktopBuyer: boolean;
  proStatus: "active" | "trialing" | "ended" | "none";
};

export type Contact = {
  externalId: string;
  email: string;
  /** Empty when the account has no name. */
  firstName: string;
  attributes: ContactAttributes;
};

export type ContactResult =
  | { kind: "ok" }
  /** No contact has this external id. */
  | { kind: "not_found" }
  /** Safe to try again later. */
  | { kind: "retry"; status: number | null; code: string; retryAfterMs: number | null }
  /** Sequenzy refused the request; repeating it will not help. */
  | { kind: "refused"; status: number; code: string };

export type CreateContact = {
  contact: Contact;
  tags: string[];
  /** Sequenzy list ids. Null leaves the workspace's default lists. */
  lists: string[] | null;
  /**
   * The account's signup time, for accounts that existed before marketing email.
   * Sequenzy then skips sequence enrollment, so a backfill sends no welcome mail.
   */
  createdAt: Date | null;
};

export type ContactsClient = {
  create(input: CreateContact): Promise<ContactResult>;
  /** Email, name and attributes. `reactivate` also sets the status back to active. */
  update(input: { contact: Contact; reactivate: boolean }): Promise<ContactResult>;
  /** `email`, when verified, keeps an unsubscribed contact's address current. */
  unsubscribe(input: { externalId: string; email: string | null }): Promise<ContactResult>;
  remove(externalId: string): Promise<ContactResult>;
};

export type SequenzyContactsOptions = {
  apiKey: string;
  /** `https://api.sequenzy.com/api/v1` unless a test points it elsewhere. */
  baseUrl?: string;
  timeoutMs?: number;
  fetch?: typeof fetch;
};

function retryAfter(res: Response): number | null {
  const header = res.headers.get("retry-after");
  if (header === null) return null;
  const seconds = Number(header);
  const ms = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(header) - Date.now();
  return Number.isFinite(ms) && ms >= 0 ? ms : null;
}

async function errorCode(res: Response): Promise<string> {
  try {
    const body: unknown = await res.json();
    if (typeof body === "object" && body !== null) {
      if ("code" in body && typeof body.code === "string") return body.code;
      if (
        "error" in body &&
        typeof body.error === "object" &&
        body.error !== null &&
        "code" in body.error &&
        typeof body.error.code === "string"
      )
        return body.error.code;
    }
  } catch {
    // The status alone decides.
  }
  return `http_${res.status}`;
}

export function sequenzyContacts(options: SequenzyContactsOptions): ContactsClient {
  const doFetch = options.fetch ?? fetch;
  const base = (options.baseUrl ?? "https://api.sequenzy.com/api/v1").replace(/\/$/, "");
  const external = (id: string) =>
    `${base}/subscribers/external?externalId=${encodeURIComponent(id)}`;

  async function call(
    method: "POST" | "PATCH" | "DELETE",
    url: string,
    body: unknown,
  ): Promise<ContactResult> {
    let res: Response;
    try {
      res = await doFetch(url, {
        method,
        headers: {
          authorization: `Bearer ${options.apiKey}`,
          ...(body === undefined ? {} : { "content-type": "application/json" }),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        signal: AbortSignal.timeout(options.timeoutMs ?? 10_000),
      });
    } catch (e) {
      const timeout = e instanceof Error && (e.name === "TimeoutError" || e.name === "AbortError");
      return {
        kind: "retry",
        status: null,
        code: timeout ? "timeout" : "network_error",
        retryAfterMs: null,
      };
    }
    if (res.ok) {
      await res.body?.cancel();
      return { kind: "ok" };
    }
    if (res.status === 404) {
      await res.body?.cancel();
      return { kind: "not_found" };
    }
    const code = await errorCode(res);
    if (res.status === 429 || res.status >= 500 || res.status === 408)
      return { kind: "retry", status: res.status, code, retryAfterMs: retryAfter(res) };
    return { kind: "refused", status: res.status, code };
  }

  return {
    create: ({ contact, tags, lists, createdAt }) =>
      call("POST", `${base}/subscribers`, {
        email: contact.email,
        externalId: contact.externalId,
        ...(contact.firstName ? { firstName: contact.firstName } : {}),
        status: "active",
        tags,
        ...(lists ? { lists } : {}),
        customAttributes: contact.attributes,
        ...(createdAt ? { createdAt: createdAt.toISOString() } : {}),
        // An existing contact with this address keeps its status: Sequenzy never
        // reactivates an unsubscribed contact through this route.
        duplicateStrategy: "merge",
      }),
    update: ({ contact, reactivate }) =>
      call("PATCH", external(contact.externalId), {
        email: contact.email,
        // Always sent, so clearing the name in convt clears it here too.
        firstName: contact.firstName,
        customAttributes: contact.attributes,
        customAttributesStrategy: "merge",
        ...(reactivate ? { status: "active" } : {}),
      }),
    unsubscribe: ({ externalId, email }) =>
      call("PATCH", external(externalId), {
        status: "unsubscribed",
        ...(email ? { email } : {}),
      }),
    remove: (externalId) => call("DELETE", external(externalId), undefined),
  };
}
