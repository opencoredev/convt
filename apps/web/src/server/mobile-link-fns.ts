// The server function behind the mobile download-link card. The logic and its
// limits are in ./mobile-link.ts.

import { consumeSendBucket, releaseSendBucket } from "@convt/db";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader, setResponseHeader } from "@tanstack/react-start/server";

import { requestContext } from "./context";
import { sendMail } from "./mail";
import { parseMobileLinkInput, requestMobileLink, type MobileLinkResult } from "./mobile-link";

export const sendMobileDownloadLink = createServerFn({ method: "POST" })
  .validator(parseMobileLinkInput)
  .handler(async ({ data, context }): Promise<MobileLinkResult> => {
    setResponseHeader("cache-control", "private, no-store");
    const { scope, appEnv } = requestContext(context);
    const now = new Date();
    return requestMobileLink(
      { email: data.email, ip: getRequestHeader("cf-connecting-ip")?.trim() || "unknown" },
      {
        consume: (key, windowMs) => consumeSendBucket(scope.db, key, windowMs, now),
        release: (key) => releaseSendBucket(scope.db, key, now),
        send: (message) => sendMail(appEnv.mail, message),
        siteUrl: appEnv.authUrl,
        now,
      },
    );
  });
