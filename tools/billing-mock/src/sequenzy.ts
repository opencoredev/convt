// Sequenzy's subscriber routes that convt-billing calls for campaign email:
// POST /api/v1/subscribers and PATCH or DELETE /api/v1/subscribers/external.
// In memory, keyed by external id. Like Sequenzy, a create never reactivates an
// unsubscribed contact, and only an explicit `status: active` does.

export type MockContact = {
  externalId: string;
  email: string;
  firstName: string;
  status: "active" | "unsubscribed";
  tags: string[];
  lists: string[];
  attributes: Record<string, unknown>;
  createdAt: string | null;
};

const object = (v: unknown): Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
const strings = (v: unknown) => (Array.isArray(v) ? v.filter((s) => typeof s === "string") : []);
const json = (status: number, body: unknown) => Response.json(body, { status });

export function createSequenzy(options: { apiKey: string }) {
  const contacts = new Map<string, MockContact>();
  let requests = 0;

  function byEmail(email: string) {
    return [...contacts.values()].find((c) => c.email === email) ?? null;
  }

  async function handle(req: Request, url: URL): Promise<Response | null> {
    if (!url.pathname.startsWith("/api/v1/subscribers")) return null;
    requests++;
    if (req.headers.get("authorization") !== `Bearer ${options.apiKey}`)
      return json(401, { success: false, code: "UNAUTHORIZED" });
    if (url.pathname === "/api/v1/subscribers" && req.method === "POST") {
      const body = object(await req.json());
      const email = typeof body.email === "string" ? body.email : "";
      const externalId = typeof body.externalId === "string" ? body.externalId : "";
      if (!email.includes("@") || !externalId)
        return json(400, { success: false, code: "VALIDATION_ERROR" });
      const byId = contacts.get(externalId);
      const byMail = byEmail(email);
      if (byId && byMail && byId !== byMail) return json(409, { success: false, code: "CONFLICT" });
      const existing = byId ?? byMail;
      if (existing) {
        contacts.delete(existing.externalId);
        contacts.set(externalId, {
          ...existing,
          externalId,
          attributes: { ...object(body.customAttributes), ...existing.attributes },
        });
        return json(200, { success: true, subscriber: { created: false, updated: true } });
      }
      contacts.set(externalId, {
        externalId,
        email,
        firstName: typeof body.firstName === "string" ? body.firstName : "",
        status: "active",
        tags: strings(body.tags),
        lists: strings(body.lists),
        attributes: object(body.customAttributes),
        createdAt: typeof body.createdAt === "string" ? body.createdAt : null,
      });
      return json(200, { success: true, subscriber: { created: true, updated: false } });
    }
    if (url.pathname === "/api/v1/subscribers/external") {
      const id = url.searchParams.get("externalId") ?? "";
      const contact = contacts.get(id);
      if (!contact) return json(404, { success: false, code: "NOT_FOUND" });
      if (req.method === "DELETE") {
        contacts.delete(id);
        return json(200, { success: true });
      }
      if (req.method === "PATCH") {
        const body = object(await req.json());
        if (typeof body.email === "string") contact.email = body.email;
        if (typeof body.firstName === "string") contact.firstName = body.firstName;
        if (body.status === "active" || body.status === "unsubscribed")
          contact.status = body.status;
        if ("customAttributes" in body)
          contact.attributes =
            body.customAttributesStrategy === "merge"
              ? { ...contact.attributes, ...object(body.customAttributes) }
              : object(body.customAttributes);
        return json(200, { success: true });
      }
    }
    return json(404, { success: false, code: "NOT_FOUND" });
  }

  return {
    handle,
    contacts,
    get requests() {
      return requests;
    },
  };
}
