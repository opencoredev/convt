// The server functions behind the mobile download-link card and the launch list's
// unsubscribe page. The logic and its limits are in ./mobile-link.ts.

import { consumeSendBucket, joinLaunchList, leaveLaunchList, releaseSendBucket } from "@convt/db";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader, setResponseHeader } from "@tanstack/react-start/server";

import { requestContext } from "./context";
import { sendMail } from "./mail";
import {
  hashUnsubscribeToken,
  parseMobileLinkInput,
  parseUnsubscribeInput,
  requestMobileLink,
  type MobileLinkResult,
} from "./mobile-link";

export const sendMobileDownloadLink = createServerFn({ method: "POST" })
  .validator(parseMobileLinkInput)
  .handler(async ({ data, context }): Promise<MobileLinkResult> => {
    setResponseHeader("cache-control", "private, no-store");
    const { scope, appEnv } = requestContext(context);
    const now = new Date();
    return requestMobileLink(
      {
        email: data.email,
        source: data.source,
        ip: getRequestHeader("cf-connecting-ip")?.trim() || "unknown",
      },
      {
        consume: (key, windowMs) => consumeSendBucket(scope.db, key, windowMs, now),
        release: (key) => releaseSendBucket(scope.db, key, now),
        join: (entry) => joinLaunchList(scope.db, { ...entry, now }),
        send: (message) => sendMail(appEnv.mail, message),
        siteUrl: appEnv.authUrl,
        unsubscribeSecret: appEnv.authSecret,
        now,
      },
    );
  });

/** Deletes the address the token belongs to. `removed: false` when no address matches. */
export const unsubscribeLaunchList = createServerFn({ method: "POST" })
  .validator(parseUnsubscribeInput)
  .handler(async ({ data, context }): Promise<{ removed: boolean }> => {
    setResponseHeader("cache-control", "private, no-store");
    const { scope } = requestContext(context);
    return { removed: await leaveLaunchList(scope.db, await hashUnsubscribeToken(data.token)) };
  });
