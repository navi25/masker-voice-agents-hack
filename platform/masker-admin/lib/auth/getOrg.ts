import { auth } from "@clerk/nextjs/server";
import { db, orgs } from "@/lib/db";
import { eq } from "drizzle-orm";
import type { Org } from "@/lib/db/schema";

/**
 * Returns the authed user's org, or null if unauthenticated / no org yet.
 * Use in API route handlers.
 */
export async function getAuthedOrg(): Promise<{ userId: string; org: Org } | null> {
  const { userId } = await auth();
  if (!userId) return null;

  const [org] = await db.select().from(orgs).where(eq(orgs.ownerId, userId)).limit(1);
  if (!org) return null;

  return { userId, org };
}

export async function getUserId(): Promise<string | null> {
  const { userId } = await auth();
  return userId;
}
