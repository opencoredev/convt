// Data access for the dashboard and account pages. Each function is a server
// function (src/server/account-fns.ts) that checks the session and reads only the
// signed-in user's rows.

import type { SessionInfo } from "#/server/session";
import {
  fetchAccountSettings,
  fetchApiOverview,
  fetchBilling,
  fetchLicenseKey,
  fetchLicenses,
  fetchOverview,
} from "#/server/account-fns";

import type { Account } from "./types";

export const getOverview = () => fetchOverview();

export const getLicenses = () => fetchLicenses();

export const getBilling = () => fetchBilling();

export const getApiOverview = () => fetchApiOverview();

export const getAccountSettings = () => fetchAccountSettings();

/** The full license key, fetched only when the user copies or activates it. */
export const getLicenseKey = async (id: string) => (await fetchLicenseKey({ data: { id } })).token;

/** The signed-in account for the header, from the session the shell already loaded. */
export function accountFromSession(session: SessionInfo): Account {
  const { user } = session;
  return {
    name: user.name.trim() || user.email.split("@")[0],
    email: user.email,
    emailVerified: user.emailVerified,
    avatarUrl: user.image,
  };
}
