// A small OAuth 2 and OpenID Connect server that stands in for GitHub and Google in
// local development and tests. It is never deployed: the web app refuses it unless
// ENV is not production and its server-side URL is loopback.
//
// GitHub:  /github/authorize, /github/token, /github/user, /github/user/emails
// Google:  /google/.well-known/openid-configuration, /google/authorize,
//          /google/token (with a signed ID token), /google/userinfo, /google/jwks

import { SignJWT, exportJWK, generateKeyPair } from "jose";

import { errorIdentity, identities, type Identity } from "./identities";

type Grant = {
  identity: Identity;
  clientId: string;
  redirectUri: string;
  nonce: string | null;
  challenge: string | null;
};

export type MockOptions = {
  /** Where the server listens, for the token, userinfo and JWKS endpoints. */
  internalUrl: string;
  /** Where a browser reaches the authorize page. Defaults to internalUrl. */
  publicUrl?: string;
};

const html = (body: string, status = 200) =>
  new Response(`<!doctype html><meta charset="utf-8"><title>OAuth mock</title>${body}`, {
    status,
    headers: { "content-type": "text/html; charset=utf-8" },
  });

const escape = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

async function s256(verifier: string): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)),
  );
  return btoa(String.fromCharCode(...digest))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

export async function createMock(options: MockOptions) {
  const internal = options.internalUrl.replace(/\/$/, "");
  const publicBase = (options.publicUrl ?? options.internalUrl).replace(/\/$/, "");
  const issuer = `${internal}/google`;
  const { publicKey, privateKey } = await generateKeyPair("RS256");
  const jwk = { ...(await exportJWK(publicKey)), kid: "mock-1", alg: "RS256", use: "sig" };
  const grants = new Map<string, Grant>();
  const tokens = new Map<string, Identity>();

  // An overridden email is a different person, so it gets its own provider id.
  function withEmail(identity: Identity, email: string | null): Identity {
    if (!email) return identity;
    const suffix = [...email].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7) % 1_000_000;
    if (identity.provider === "google")
      return { ...identity, sub: `${identity.sub}-${suffix}`, email };
    return {
      ...identity,
      id: identity.id * 1_000_000 + suffix,
      publicEmail: identity.publicEmail ? email : null,
      emails: identity.emails.length
        ? identity.emails.map((e) => (e.primary ? { ...e, email } : e))
        : [{ email, primary: true, verified: true }],
    };
  }

  function authorize(provider: "github" | "google", url: URL): Response {
    const redirectUri = url.searchParams.get("redirect_uri");
    const state = url.searchParams.get("state");
    if (!redirectUri || !state) return html("<p>missing redirect_uri or state</p>", 400);
    const back = new URL(redirectUri);
    back.searchParams.set("state", state);
    const pick = url.searchParams.get("identity");
    if (!pick) {
      const choices = Object.entries(identities)
        .filter(([, i]) => i.provider === provider)
        .map(([name]) => {
          const link = new URL(url);
          link.searchParams.set("identity", name);
          return `<li><a href="${escape(link.toString())}">${escape(name)}</a></li>`;
        });
      const deny = new URL(url);
      deny.searchParams.set("identity", errorIdentity);
      choices.push(
        `<li><a href="${escape(deny.toString())}">${errorIdentity} (deny access)</a></li>`,
      );
      return html(`<h1>Sign in to the ${provider} mock</h1><ul>${choices.join("")}</ul>`);
    }
    if (pick === errorIdentity) {
      back.searchParams.set("error", "access_denied");
      return Response.redirect(back.toString(), 302);
    }
    const identity = identities[pick];
    if (!identity || identity.provider !== provider)
      return html(`<p>unknown identity ${escape(pick)}</p>`, 400);
    const code = crypto.randomUUID();
    grants.set(code, {
      identity: withEmail(identity, url.searchParams.get("email")),
      clientId: url.searchParams.get("client_id") ?? "",
      redirectUri,
      nonce: url.searchParams.get("nonce"),
      challenge: url.searchParams.get("code_challenge"),
    });
    back.searchParams.set("code", code);
    return Response.redirect(back.toString(), 302);
  }

  async function token(provider: "github" | "google", req: Request): Promise<Response> {
    const form = await req.formData();
    const code = String(form.get("code") ?? "");
    const grant = grants.get(code);
    grants.delete(code);
    if (!grant || grant.identity.provider !== provider)
      return Response.json({ error: "invalid_grant" }, { status: 400 });
    if (String(form.get("redirect_uri") ?? "") !== grant.redirectUri)
      return Response.json(
        { error: "invalid_grant", error_description: "redirect_uri" },
        { status: 400 },
      );
    if (
      grant.challenge &&
      (await s256(String(form.get("code_verifier") ?? ""))) !== grant.challenge
    )
      return Response.json({ error: "invalid_grant", error_description: "pkce" }, { status: 400 });
    const accessToken = `mock_${crypto.randomUUID()}`;
    tokens.set(accessToken, grant.identity);
    const body: Record<string, unknown> = {
      access_token: accessToken,
      token_type: "bearer",
      expires_in: 3600,
      scope: provider === "github" ? "user:email" : "openid email profile",
    };
    if (grant.identity.provider === "google") {
      const i = grant.identity;
      body.id_token = await new SignJWT({
        email: i.email,
        email_verified: i.email_verified,
        name: i.name,
        ...(i.hd ? { hd: i.hd } : {}),
        ...(grant.nonce ? { nonce: grant.nonce } : {}),
      })
        .setProtectedHeader({ alg: "RS256", kid: jwk.kid })
        .setIssuer(issuer)
        .setAudience(grant.clientId)
        .setSubject(i.sub)
        .setIssuedAt()
        .setExpirationTime("10m")
        .sign(privateKey);
    }
    return Response.json(body);
  }

  function bearer(req: Request): Identity | null {
    const auth = req.headers.get("authorization") ?? "";
    return tokens.get(auth.replace(/^Bearer /i, "")) ?? null;
  }

  async function fetch(req: Request): Promise<Response> {
    const url = new URL(req.url);
    const path = url.pathname;
    if (path === "/")
      return html(
        '<p>convt OAuth mock: <a href="/github/authorize">github</a>, <a href="/google/authorize">google</a></p>',
      );
    if (path === "/health") return new Response("ok");
    if (path === "/github/authorize") return authorize("github", url);
    if (path === "/google/authorize") return authorize("google", url);
    if (path === "/github/token" && req.method === "POST") return token("github", req);
    if (path === "/google/token" && req.method === "POST") return token("google", req);
    if (path === "/google/.well-known/openid-configuration")
      return Response.json({
        issuer,
        authorization_endpoint: `${publicBase}/google/authorize`,
        token_endpoint: `${internal}/google/token`,
        userinfo_endpoint: `${internal}/google/userinfo`,
        jwks_uri: `${internal}/google/jwks`,
        id_token_signing_alg_values_supported: ["RS256"],
        response_types_supported: ["code"],
        subject_types_supported: ["public"],
      });
    if (path === "/google/jwks") return Response.json({ keys: [jwk] });
    const who = bearer(req);
    if (path === "/github/user") {
      if (who?.provider !== "github")
        return Response.json({ message: "Bad credentials" }, { status: 401 });
      return Response.json({
        id: who.id,
        login: who.login,
        name: who.name,
        email: who.publicEmail,
        avatar_url: null,
      });
    }
    if (path === "/github/user/emails") {
      if (who?.provider !== "github")
        return Response.json({ message: "Bad credentials" }, { status: 401 });
      return Response.json(
        who.emails.map((e) => ({ ...e, visibility: e.primary ? "private" : null })),
      );
    }
    if (path === "/google/userinfo") {
      if (who?.provider !== "google")
        return Response.json({ error: "invalid_token" }, { status: 401 });
      return Response.json({
        sub: who.sub,
        email: who.email,
        email_verified: who.email_verified,
        name: who.name,
        hd: who.hd,
      });
    }
    return new Response("not found", { status: 404 });
  }

  return { fetch, issuer };
}
