// Data access for the dashboard and account pages. Today every function returns
// placeholder data; when the account API exists (plan P6 to P9), replace the bodies
// with real requests and keep the signatures.

import {
  placeholderApi,
  placeholderBilling,
  placeholderLicenses,
  placeholderMacs,
  placeholderOverview,
  placeholderSettings,
} from "./placeholder";
import type { AccountSettings, ApiOverview, Billing, License, Mac, Overview } from "./types";

export async function getOverview(): Promise<Overview> {
  return placeholderOverview;
}

export async function getLicenses(): Promise<{ licenses: License[]; macs: Mac[] }> {
  return { licenses: placeholderLicenses, macs: placeholderMacs };
}

export async function getBilling(): Promise<Billing> {
  return placeholderBilling;
}

export async function getApiOverview(): Promise<ApiOverview> {
  return placeholderApi;
}

export async function getAccountSettings(): Promise<AccountSettings> {
  return placeholderSettings;
}

/** The signed-in account for the header. Placeholder until sessions exist. */
export async function getAccount() {
  return placeholderOverview.account;
}
