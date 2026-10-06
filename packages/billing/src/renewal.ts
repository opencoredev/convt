// Pro renewal for the desktop app (P8). The site authenticates the device token and
// asks for the account's current Pro key through BillingRpc.currentProKey. The key
// is the newest unrevoked paid Pro key, whatever the subscription's status now: a
// lapsed subscription's last key stays valid for every build it covers, and the app
// keeps whichever stored key covers more.

import { and, desc, eq, isNull } from "drizzle-orm";

import { schema as t, type Db } from "@convt/db";

export type CurrentProKey = { key: string; updatesUntil: string } | null;

export async function currentProKey(db: Db, userId: string): Promise<CurrentProKey> {
  const [row] = await db
    .select({ key: t.licenses.token, updatesUntil: t.licenses.updatesUntil })
    .from(t.licenses)
    .where(
      and(
        eq(t.licenses.userId, userId),
        eq(t.licenses.plan, "pro"),
        eq(t.licenses.trial, false),
        isNull(t.licenses.revokedAt),
      ),
    )
    .orderBy(desc(t.licenses.updatesUntil), desc(t.licenses.createdAt))
    .limit(1);
  return row ?? null;
}
