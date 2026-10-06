// Fixture identities. Tests and the e2e scripts pick one with `?identity=` on the
// authorize URL; without it the authorize page lists them. `?email=` overrides the
// address for tests that need a specific one.

export type GoogleIdentity = {
  provider: "google";
  sub: string;
  name: string;
  email: string;
  email_verified: boolean;
  hd?: string;
  picture?: string;
};

export type GithubIdentity = {
  provider: "github";
  id: number;
  login: string;
  name: string;
  /** The public profile email, which Better Auth reads before the primary one. */
  publicEmail: string | null;
  emails: Array<{ email: string; primary: boolean; verified: boolean }>;
};

export type Identity = GoogleIdentity | GithubIdentity;

export const identities: Record<string, Identity> = {
  "google-gmail": {
    provider: "google",
    sub: "g-1001",
    name: "Gail Gmail",
    email: "gail.mock@gmail.com",
    email_verified: true,
  },
  "google-workspace": {
    provider: "google",
    sub: "g-1002",
    name: "Walt Workspace",
    email: "walt@workspace.test",
    email_verified: true,
    hd: "workspace.test",
  },
  // A verified flag with no hosted domain: Google once checked this address but
  // does not control it, so convt does not count it as verified.
  "google-thirdparty": {
    provider: "google",
    sub: "g-1003",
    name: "Theo Thirdparty",
    email: "theo@thirdparty.test",
    email_verified: true,
  },
  "google-unverified": {
    provider: "google",
    sub: "g-1004",
    name: "Una Unverified",
    email: "una@unverified.test",
    email_verified: false,
  },
  "github-verified": {
    provider: "github",
    id: 2001,
    login: "octo-verified",
    name: "Octo Verified",
    publicEmail: null,
    emails: [{ email: "octo@github-user.test", primary: true, verified: true }],
  },
  "github-public-differs": {
    provider: "github",
    id: 2002,
    login: "octo-public",
    name: "Octo Public",
    publicEmail: "public@github-user.test",
    emails: [
      { email: "primary@github-user.test", primary: true, verified: true },
      { email: "public@github-user.test", primary: false, verified: false },
    ],
  },
  "github-no-email": {
    provider: "github",
    id: 2003,
    login: "octo-noemail",
    name: "Octo No Email",
    publicEmail: null,
    emails: [],
  },
  // Linked to pro@convt.test by the seed (packages/db/src/seed.ts).
  "github-pro": {
    provider: "github",
    id: 100200300,
    login: "leoisadev1",
    name: "Leo",
    publicEmail: null,
    emails: [{ email: "pro@convt.test", primary: true, verified: true }],
  },
};

/** Not an identity: the authorize step answers with `error=access_denied`. */
export const errorIdentity = "error";
