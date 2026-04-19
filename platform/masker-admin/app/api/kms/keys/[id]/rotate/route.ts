import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, kmsKeys } from "@/lib/db";
import { eq, and } from "drizzle-orm";

export async function POST(_req: NextRequest, { params }: { params: { id: string } }) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const [updated] = await db.update(kmsKeys)
    .set({ lastRotatedAt: new Date(), status: "active" })
    .where(and(eq(kmsKeys.id, params.id), eq(kmsKeys.orgId, ctx.org.id)))
    .returning();

  if (!updated) return NextResponse.json({ error: "Not found" }, { status: 404 });
  return NextResponse.json(updated);
}
