import { NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, auditLogs } from "@/lib/db";
import { eq, desc } from "drizzle-orm";

export async function GET() {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const logs = await db.select({
    useCase: auditLogs.useCase,
    status: auditLogs.status,
    createdAt: auditLogs.createdAt,
  }).from(auditLogs)
    .where(eq(auditLogs.orgId, ctx.org.id))
    .orderBy(desc(auditLogs.createdAt));

  if (!logs.length) return NextResponse.json([]);

  const groups: Record<string, { count: number; latest: Date }> = {};
  for (const log of logs) {
    if (!groups[log.useCase]) groups[log.useCase] = { count: 0, latest: log.createdAt };
    groups[log.useCase].count++;
  }

  const reports = Object.entries(groups).map(([useCase, { count, latest }], i) => ({
    id: `rpt-${i}`,
    name: `${useCase} Audit`,
    useCase,
    dateRange: `Up to ${new Date(latest).toLocaleDateString()}`,
    generatedAt: latest,
    generatedBy: "system",
    status: "ready",
    sessionCount: count,
    formats: ["PDF", "JSON", "CSV"],
  }));

  return NextResponse.json(reports);
}
