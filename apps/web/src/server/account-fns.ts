// Server functions for the dashboard and settings pages. Each runs `authed`, so the
// user id always comes from the verified session cookie, never from the request.

import {
  activeApiKeys,
  activeDevices,
  apiConversionsThisMonth,
  apiUsagePerDay,
  failedApiJobs,
  getLicenseToken,
  openApiCheckout,
  openDeletion,
  revokeDevice as revokeDeviceRow,
  revokeOtherSessions as revokeOtherSessionsRows,
  revokeSession as revokeSessionRow,
  userAccounts,
  userInvoices,
  userLicenses,
  userPolarCustomerId,
  userSessions,
  userSubscriptions,
} from "@convt/db/queries";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeaders } from "@tanstack/react-start/server";

import { availableProviders } from "./env";
import { billing } from "./billing";
import { signedInUser } from "./context";
import { authed } from "./session";
import {
  apiEnrollment,
  apiKeyView,
  billingView,
  licensesView,
  overviewView,
  settingsView,
  visibleMethods,
} from "./views";

export const fetchOverview = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId } }) => {
    const now = new Date();
    const [user, subscriptions, licenses, devices, apiMonth, keys] = [
      await signedInUser(db, userId),
      await userSubscriptions(db, userId),
      await userLicenses(db, userId),
      await activeDevices(db, userId),
      await apiConversionsThisMonth(db, userId, now),
      await activeApiKeys(db, userId),
    ];
    return overviewView({
      user,
      subscriptions,
      licenses,
      devices,
      apiMonth,
      apiKeyCount: keys.length,
      now,
    });
  });

export const fetchLicenses = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId } }) => {
    const subscriptions = await userSubscriptions(db, userId);
    const licenses = await userLicenses(db, userId);
    const devices = await activeDevices(db, userId);
    return licensesView(subscriptions, licenses, devices, new Date());
  });

/** The full token, for "Copy key" and "Activate on this computer". Never in page HTML. */
export const fetchLicenseKey = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { id: string }) => {
    if (typeof data?.id !== "string" || data.id.length > 64) throw new Error("bad license id");
    return data;
  })
  .handler(async ({ data, context: { db, userId } }) => {
    const token = await getLicenseToken(db, userId, data.id);
    if (!token) throw new Error("License not found");
    return { token };
  });

export const fetchBilling = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId, appEnv } }) => {
    const now = new Date();
    const user = await signedInUser(db, userId);
    // The provider holds cards; ask convt-billing, and show none if it cannot answer.
    const card = await billing()
      .card(userId)
      .catch(() => null);
    return {
      sales: appEnv.sales,
      ...billingView({
        user,
        subscriptions: await userSubscriptions(db, userId),
        licenses: await userLicenses(db, userId),
        invoices: await userInvoices(db, userId),
        card,
        openApiCheckout: (await openApiCheckout(db, userId, now)) !== null,
        now,
        polarCustomerId: await userPolarCustomerId(db, userId),
      }),
    };
  });

export const fetchApiOverview = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId, appEnv } }) => {
    const now = new Date();
    const perDay = await apiUsagePerDay(db, userId, now);
    const month = await apiConversionsThisMonth(db, userId, now);
    const enrollment = apiEnrollment(
      await userSubscriptions(db, userId),
      (await openApiCheckout(db, userId, now)) !== null,
      now,
    );
    // Only asked when it matters: an account that could start enrolling.
    const multipleAllowed =
      appEnv.sales === "all" && (enrollment.state === "none" || enrollment.state === "ended")
        ? await billing()
            .multipleSubscriptionsAllowed()
            .catch(() => null)
        : true;
    return {
      sales: appEnv.sales,
      enrollment,
      enrollBlocked: multipleAllowed === false,
      thisMonth: month.count,
      since: month.since.toISOString().slice(0, 10),
      last30Days: perDay.reduce((n, d) => n + d.conversions, 0),
      failed: await failedApiJobs(db, userId, now),
      perDay,
      keys: (await activeApiKeys(db, userId)).map((k) => apiKeyView(k, now)),
    };
  });

export const fetchAccountSettings = createServerFn({ method: "GET" })
  .middleware([authed])
  .handler(async ({ context: { db, userId, sessionId, appEnv } }) => {
    const now = new Date();
    const settings = settingsView({
      user: await signedInUser(db, userId),
      accounts: await userAccounts(db, userId),
      sessions: await userSessions(db, userId, now),
      devices: await activeDevices(db, userId),
      currentSessionId: sessionId,
      deletion: await openDeletion(db, userId),
      now,
    });
    return {
      ...settings,
      methods: visibleMethods(settings.methods, availableProviders(appEnv)),
      // Null when convt-billing cannot answer; the page says so instead of guessing.
      marketing: await billing()
        .marketingPreference(userId)
        .catch(() => null),
    };
  });

export const saveName = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { name: string }) => {
    const name = typeof data?.name === "string" ? data.name.trim() : "";
    if (name.length > 100) throw new Error("That name is too long.");
    return { name };
  })
  .handler(async ({ data, context: { auth } }) => {
    await auth.api.updateUser({
      body: { name: data.name },
      headers: getRequestHeaders() as unknown as Headers,
    });
    return { ok: true };
  });

export const endSession = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: { id: string; type: "web" | "device" }) => {
    if (typeof data?.id !== "string" || (data.type !== "web" && data.type !== "device"))
      throw new Error("bad session");
    return data;
  })
  .handler(async ({ data, context: { db, userId, sessionId } }) => {
    if (data.type === "web") {
      if (data.id === sessionId) throw new Error("Use Sign out to end this browser's session.");
      return { ok: await revokeSessionRow(db, userId, data.id) };
    }
    return { ok: await revokeDeviceRow(db, userId, data.id, new Date()) };
  });

export const endOtherSessions = createServerFn({ method: "POST" })
  .middleware([authed])
  .handler(async ({ context: { db, userId, sessionId } }) =>
    revokeOtherSessionsRows(db, userId, sessionId, new Date()),
  );
