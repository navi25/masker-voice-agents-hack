import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, kmsKeys } from "@/lib/db";
import { eq, and } from "drizzle-orm";

export async function PATCH(req: NextRequest, { params }: { params: { id: string } }) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const body = await req.json() as { status?: string };

  const [updated] = await db.update(kmsKeys)
    .set(body)
    .where(and(eq(kmsKeys.id, params.id), eq(kmsKeys.orgId, ctx.org.id)))
    .returning();

  if (!updated) return NextResponse.json({ error: "Not found" }, { status: 404 });
  return NextResponse.json(updated);
}
