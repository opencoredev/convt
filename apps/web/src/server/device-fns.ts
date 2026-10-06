// The /device page's server function. `authed` takes the user from the session
// cookie; the page passes only what the app put in the URL.

import { createServerFn } from "@tanstack/react-start";

import { approveDevice, parseDeviceRequest } from "./device-auth";
import { authed } from "./session";

export const approveDeviceSignIn = createServerFn({ method: "POST" })
  .middleware([authed])
  .validator((data: Record<string, unknown>) => {
    const req = parseDeviceRequest(data ?? {});
    if (!req) throw new Error("bad device request");
    return req;
  })
  .handler(async ({ data, context: { db, userId } }) =>
    approveDevice(db, userId, data, new Date()),
  );
