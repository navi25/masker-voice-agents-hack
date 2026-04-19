import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, auditLogs } from "@/lib/db";
import { eq, and, desc } from "drizzle-orm";

export async function GET(req: NextRequest) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const { searchParams } = req.nextUrl;
  const status = searchParams.get("status");
  const risk = searchParams.get("risk");
  const channel = searchParams.get("channel");
  const limit = parseInt(searchParams.get("limit") ?? "100", 10);

  const conditions = [eq(auditLogs.orgId, ctx.org.id)];
  if (status) conditions.push(eq(auditLogs.status, status));
  if (risk) conditions.push(eq(auditLogs.riskLevel, risk));
  if (channel) conditions.push(eq(auditLogs.channel, channel));

  const logs = await db.select().from(auditLogs)
    .where(and(...conditions))
    .orderBy(desc(auditLogs.createdAt))
    .limit(limit);

  return NextResponse.json(logs);
}
