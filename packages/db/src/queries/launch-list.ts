import { eq, sql } from "drizzle-orm";

import type { Db } from "../client";
import { launchList, type launchListSources } from "../schema";

export type LaunchListSource = (typeof launchListSources)[number];

/**
 * Adds an address to the launch list, or refreshes it: the source and the last
 * request time change, the first consent time stays. `email` must already be
 * trimmed and lowercased (the table checks).
 */
export async function joinLaunchList(
  db: Db,
  entry: { email: string; source: LaunchListSource; unsubscribeTokenHash: string; now: Date },
): Promise<void> {
  await db
    .insert(launchList)
    .values({
      email: entry.email,
      source: entry.source,
      unsubscribeTokenHash: entry.unsubscribeTokenHash,
      consentedAt: entry.now,
      lastRequestedAt: entry.now,
    })
    .onConflictDoUpdate({
      target: launchList.email,
      set: {
        source: sql`excluded.source`,
        unsubscribeTokenHash: sql`excluded.unsubscribe_token_hash`,
        lastRequestedAt: sql`excluded.last_requested_at`,
      },
    });
}

/** Deletes the address whose current unsubscribe token hashes to this. True when one was deleted. */
export async function leaveLaunchList(db: Db, unsubscribeTokenHash: string): Promise<boolean> {
  const rows = await db
    .delete(launchList)
    .where(eq(launchList.unsubscribeTokenHash, unsubscribeTokenHash))
    .returning({ email: launchList.email });
  return rows.length > 0;
}
