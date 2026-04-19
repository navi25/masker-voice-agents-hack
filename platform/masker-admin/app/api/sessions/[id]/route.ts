import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, auditLogs } from "@/lib/db";
import { eq, and } from "drizzle-orm";

export async function GET(_req: NextRequest, { params }: { params: { id: string } }) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const [log] = await db.select().from(auditLogs)
    .where(and(eq(auditLogs.sessionId, params.id), eq(auditLogs.orgId, ctx.org.id)))
    .limit(1);

  if (!log) return NextResponse.json({ error: "Not found" }, { status: 404 });
  return NextResponse.json(log);
}
