import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, apiKeys } from "@/lib/db";
import { eq, and } from "drizzle-orm";
import { createHash, randomBytes } from "crypto";

export async function POST(_req: NextRequest, { params }: { params: { id: string } }) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const [existing] = await db.select().from(apiKeys)
    .where(and(eq(apiKeys.id, params.id), eq(apiKeys.orgId, ctx.org.id)));
  if (!existing) return NextResponse.json({ error: "Not found" }, { status: 404 });

  const envTag = existing.environment === "production" ? "live" : "test";
  const suffix = randomBytes(4).toString("hex");
  const prefix = `msk_${envTag}_${suffix}`;
  const fullKey = `${prefix}_${randomBytes(16).toString("hex")}`;
  const keyHash = createHash("sha256").update(fullKey).digest("hex");

  const [updated] = await db.update(apiKeys)
    .set({ prefix, keyHash, status: "active" })
    .where(eq(apiKeys.id, params.id))
    .returning();

  return NextResponse.json({ ...updated, full_key: fullKey });
}
