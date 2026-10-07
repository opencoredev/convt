// The /api/device handlers. The desktop app calls them with JSON and no cookies;
// they answer JSON and are never cached. See device-auth.ts for the flow.

import { billing } from "./billing";
import { requestContext } from "./context";
import {
  bearer,
  readSmallJson,
  exchangeCode,
  renewDevice,
  signOutDevice,
  startDeviceTrial,
  type DeviceResponse,
} from "./device-auth";

function respond(r: DeviceResponse): Response {
  return Response.json(r.body, {
    status: r.status,
    headers: { "cache-control": "no-store" },
  });
}

const ipOf = (request: Request) => request.headers.get("cf-connecting-ip")?.trim() || "unknown";

export type DeviceRoute = "token" | "license" | "trial" | "sign-out";

export async function handleDevice(
  route: DeviceRoute,
  request: Request,
  context: unknown,
): Promise<Response> {
  const { scope, appEnv } = requestContext(context);
  const body = await readSmallJson(request);
  if (body === "too_large") return respond({ status: 413, body: { error: "too_large" } });
  const input = body?.value ?? null;
  const now = new Date();
  const token = bearer(request.headers.get("authorization"));
  switch (route) {
    case "token":
      return respond(await exchangeCode(scope.db, input, ipOf(request), now));
    case "license":
      return respond(
        await renewDevice(
          scope.db,
          (userId, at, anyPlan) =>
            anyPlan ? billing().currentKey(userId, at) : billing().currentProKey(userId),
          token,
          input,
          ipOf(request),
          now,
        ),
      );
    case "trial":
      return respond(
        await startDeviceTrial(
          scope.db,
          {
            startTrial: (userId, deviceHash, at) => billing().startTrial(userId, deviceHash, at),
            secret: appEnv.deviceHashSecret,
          },
          token,
          input,
          ipOf(request),
          now,
        ),
      );
    case "sign-out":
      return respond(await signOutDevice(scope.db, token, now));
  }
}
