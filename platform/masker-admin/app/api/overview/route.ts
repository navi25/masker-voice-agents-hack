import { NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, auditLogs, kmsKeys, apiKeys } from "@/lib/db";
import { eq, and, gte, count, sum } from "drizzle-orm";

export async function GET() {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const { org } = ctx;
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const sevenDaysAgo = new Date(Date.now() - 7 * 86400000);

  const [
    todayLogs,
    failedLogs,
    allKmsKeys,
    activeApiKeys,
    recentLogs,
    volumeLogs,
  ] = await Promise.all([
    db.select({ count: count(), entities: sum(auditLogs.entitiesDetected) })
      .from(auditLogs)
      .where(and(eq(auditLogs.orgId, org.id), gte(auditLogs.createdAt, today))),
    db.select({ count: count() })
      .from(auditLogs)
      .where(and(eq(auditLogs.orgId, org.id), eq(auditLogs.status, "blocked"), gte(auditLogs.createdAt, today))),
    db.select({ status: kmsKeys.status }).from(kmsKeys).where(eq(kmsKeys.orgId, org.id)),
    db.select({ id: apiKeys.id }).from(apiKeys)
      .where(and(eq(apiKeys.orgId, org.id), eq(apiKeys.status, "active"))),
    db.select({
      sessionId: auditLogs.sessionId,
      status: auditLogs.status,
      riskLevel: auditLogs.riskLevel,
      createdAt: auditLogs.createdAt,
      useCase: auditLogs.useCase,
    }).from(auditLogs)
      .where(eq(auditLogs.orgId, org.id))
      .orderBy(auditLogs.createdAt)
      .limit(5),
    db.select({ createdAt: auditLogs.createdAt })
      .from(auditLogs)
      .where(and(eq(auditLogs.orgId, org.id), gte(auditLogs.createdAt, sevenDaysAgo))),
  ]);

  const kmsHealthy = allKmsKeys.every((k) => k.status === "active");

  // Build 7-day volume buckets
  const buckets: Record<string, number> = {};
  for (let i = 6; i >= 0; i--) {
    const d = new Date(Date.now() - i * 86400000);
    buckets[d.toLocaleDateString("en-US", { month: "short", day: "numeric" })] = 0;
  }
  volumeLogs.forEach((r) => {
    const label = new Date(r.createdAt).toLocaleDateString("en-US", { month: "short", day: "numeric" });
    if (label in buckets) buckets[label]++;
  });

  const recentIncidents = recentLogs
    .filter((r) => r.riskLevel === "high" || r.riskLevel === "critical")
    .map((r) => ({
      id: r.sessionId,
      summary: `${r.riskLevel} risk session — ${r.useCase}`,
      time: r.createdAt,
      risk: r.riskLevel,
    }));

  return NextResponse.json({
    metrics: {
      activePolicies: activeApiKeys.length,
      protectedSessionsToday: todayLogs[0]?.count ?? 0,
      entitiesMaskedToday: Number(todayLogs[0]?.entities ?? 0),
      failedRedactions: failedLogs[0]?.count ?? 0,
      auditReadinessScore: kmsHealthy ? 94 : 72,
      kmsHealth: kmsHealthy ? "Healthy" : "Degraded",
    },
    sessionVolume: Object.entries(buckets).map(([date, sessions]) => ({ date, sessions })),
    topEntityTypes: [],
    recentIncidents,
  });
}
