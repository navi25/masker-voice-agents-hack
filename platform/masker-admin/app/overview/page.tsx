import { PageShell } from "@/components/layout/PageShell";
import { MetricCard } from "@/components/ui/MetricCard";
import { StatusChip } from "@/components/ui/StatusChip";
import { Button } from "@/components/ui/Button";
import { ArrowRight, Download, Plus, Upload } from "lucide-react";
import Link from "next/link";
import { OverviewCharts } from "./OverviewCharts";
import { auth } from "@clerk/nextjs/server";
import { db, orgs, auditLogs, kmsKeys, apiKeys } from "@/lib/db";
import { eq, and, gte, count, sum, desc } from "drizzle-orm";

async function getOverview() {
  const { userId } = await auth();
  if (!userId) return null;

  const [org] = await db.select().from(orgs).where(eq(orgs.ownerId, userId)).limit(1);
  if (!org) return null;

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const sevenDaysAgo = new Date(Date.now() - 7 * 86400000);

  const [
    todayStats,
    failedStats,
    allKmsKeys,
    activeApiKeys,
    recentLogs,
    volumeLogs,
  ] = await Promise.all([
    db.select({ total: count(), entities: sum(auditLogs.entitiesDetected) })
      .from(auditLogs)
      .where(and(eq(auditLogs.orgId, org.id), gte(auditLogs.createdAt, today))),
    db.select({ total: count() })
      .from(auditLogs)
      .where(and(eq(auditLogs.orgId, org.id), eq(auditLogs.status, "blocked"), gte(auditLogs.createdAt, today))),
    db.select({ status: kmsKeys.status }).from(kmsKeys).where(eq(kmsKeys.orgId, org.id)),
    db.select({ id: apiKeys.id }).from(apiKeys)
      .where(and(eq(apiKeys.orgId, org.id), eq(apiKeys.status, "active"))),
    db.select({
      sessionId: auditLogs.sessionId,
      riskLevel: auditLogs.riskLevel,
      createdAt: auditLogs.createdAt,
      useCase: auditLogs.useCase,
    }).from(auditLogs)
      .where(eq(auditLogs.orgId, org.id))
      .orderBy(desc(auditLogs.createdAt))
      .limit(5),
    db.select({ createdAt: auditLogs.createdAt })
      .from(auditLogs)
      .where(and(eq(auditLogs.orgId, org.id), gte(auditLogs.createdAt, sevenDaysAgo))),
  ]);

  const kmsHealthy = allKmsKeys.every((k) => k.status === "active");

  const buckets: Record<string, number> = {};
  for (let i = 6; i >= 0; i--) {
    const d = new Date(Date.now() - i * 86400000);
    buckets[d.toLocaleDateString("en-US", { month: "short", day: "numeric" })] = 0;
  }
  volumeLogs.forEach((r) => {
    const label = new Date(r.createdAt).toLocaleDateString("en-US", { month: "short", day: "numeric" });
    if (label in buckets) buckets[label]++;
  });

  return {
    org,
    metrics: {
      activePolicies: activeApiKeys.length,
      protectedSessionsToday: todayStats[0]?.total ?? 0,
      entitiesMaskedToday: Number(todayStats[0]?.entities ?? 0),
      failedRedactions: failedStats[0]?.total ?? 0,
      auditReadinessScore: kmsHealthy ? 94 : 72,
      kmsHealth: kmsHealthy ? "Healthy" : "Degraded",
    },
    sessionVolume: Object.entries(buckets).map(([date, sessions]) => ({ date, sessions })),
    topEntityTypes: [] as { type: string; count: number }[],
    recentIncidents: recentLogs
      .filter((r) => r.riskLevel === "high" || r.riskLevel === "critical")
      .map((r) => ({
        id: r.sessionId,
        summary: `${r.riskLevel} risk — ${r.useCase}`,
        time: r.createdAt as Date,
        risk: r.riskLevel as "high" | "critical",
      })),
    recentReports: [] as { id: string; name: string; useCase: string; status: string }[],
  };
}

export default async function OverviewPage() {
  const data = await getOverview();

  if (!data) {
    return (
      <PageShell title="Overview">
        <div className="flex flex-col items-center justify-center h-[60vh] gap-4 text-center">
          <p className="text-[15px] font-medium text-[#0d0f12]">Finish setting up your workspace</p>
          <p className="text-[13px] text-[#6b7280]">Complete onboarding to see your compliance dashboard.</p>
          <Link href="/onboarding">
            <Button variant="primary">Complete setup</Button>
          </Link>
        </div>
      </PageShell>
    );
  }

  const { org, metrics: m, sessionVolume, topEntityTypes, recentIncidents, recentReports } = data;
  const hasData = m.protectedSessionsToday > 0;

  return (
    <PageShell title="Overview">
      <div className="mb-6">
        <h2 className="text-[22px] font-semibold tracking-tight text-[#0d0f12]">{org.name}</h2>
        <p className="text-[13px] text-[#6b7280] mt-1">
          {new Date().toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" })} · Production · {org.framework}
        </p>
      </div>

      <div className="grid grid-cols-3 xl:grid-cols-6 gap-3 mb-6">
        <MetricCard label="Active API Keys"       value={m.activePolicies}            sub="in production" />
        <MetricCard label="Protected Sessions"    value={m.protectedSessionsToday}    sub="today" accent={hasData ? "green" : undefined} />
        <MetricCard label="Entities Masked"       value={m.entitiesMaskedToday}       sub="today" />
        <MetricCard label="Failed Redactions"     value={m.failedRedactions}          sub="last 24h" accent={m.failedRedactions > 0 ? "red" : "green"} />
        <MetricCard label="Audit Readiness"       value={`${m.auditReadinessScore}%`} sub={org.framework + " baseline"} accent="green" />
        <MetricCard label="KMS Health"            value={m.kmsHealth}                 sub="encryption keys" accent={m.kmsHealth === "Healthy" ? "green" : "red"} />
      </div>

      {!hasData ? (
        <div className="rounded-lg border border-dashed border-[#e5e7eb] bg-[#fafafa] p-12 text-center mb-6">
          <p className="text-[15px] font-medium text-[#0d0f12] mb-2">No sessions yet</p>
          <p className="text-[13px] text-[#6b7280] mb-6 max-w-sm mx-auto">
            Integrate the Masker SDK into your voice agent and sessions will appear here in real-time.
          </p>
          <div className="flex items-center justify-center gap-3">
            <Link href="/api-keys">
              <Button variant="primary"><Plus className="w-3.5 h-3.5" /> Get API key</Button>
            </Link>
            <a href="/api/sdk/download?format=python&key=YOUR_KEY&framework=HIPAA&org=My+Org">
              <Button variant="secondary"><Download className="w-3.5 h-3.5" /> Download SDK</Button>
            </a>
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-3 gap-4 mb-4">
          <div className="col-span-2 rounded-lg border border-[#e5e7eb] bg-white p-5">
            <div className="flex items-center justify-between mb-4">
              <div>
                <div className="text-[13px] font-semibold text-[#0d0f12]">Session Volume</div>
                <div className="text-[12px] text-[#9ca3af]">Last 7 days</div>
              </div>
              <Link href="/sessions" className="text-[12px] text-[#6b7280] hover:text-[#0d0f12] flex items-center gap-1">
                View all <ArrowRight className="w-3 h-3" />
              </Link>
            </div>
            <OverviewCharts volumeData={sessionVolume} entityData={topEntityTypes} />
          </div>

          <div className="rounded-lg border border-[#e5e7eb] bg-white p-5">
            <div className="flex items-center justify-between mb-4">
              <div className="text-[13px] font-semibold text-[#0d0f12]">Recent Incidents</div>
              <Link href="/sessions" className="text-[12px] text-[#6b7280] hover:text-[#0d0f12] flex items-center gap-1">
                All <ArrowRight className="w-3 h-3" />
              </Link>
            </div>
            {recentIncidents.length === 0 ? (
              <p className="text-[12px] text-[#9ca3af] italic">No incidents today</p>
            ) : (
              <div className="flex flex-col gap-3">
                {recentIncidents.map((inc) => (
                  <div key={inc.id} className="flex flex-col gap-1 pb-3 border-b border-[#f3f4f6] last:border-0 last:pb-0">
                    <div className="flex items-center justify-between">
                      <StatusChip status={inc.risk} />
                      <time className="text-[11px] text-[#9ca3af] font-mono">
                        {new Date(inc.time).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                      </time>
                    </div>
                    <p className="text-[12px] text-[#374151] leading-snug">{inc.summary}</p>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      <div className="grid grid-cols-3 gap-4">
        <div className="col-span-2 rounded-lg border border-[#e5e7eb] bg-white p-5">
          <div className="text-[13px] font-semibold text-[#0d0f12] mb-4">Recent Audit Reports</div>
          {recentReports.length === 0 ? (
            <p className="text-[12px] text-[#9ca3af] italic">
              Reports appear here once your SDK sends sessions.{" "}
              <Link href="/audit-reports" className="text-indigo-600 hover:underline">View all</Link>
            </p>
          ) : (
            <table className="w-full text-[12px]">
              <tbody>
                {recentReports.map((r) => (
                  <tr key={r.id} className="border-b border-[#f9fafb] last:border-0">
                    <td className="py-2.5 text-[#0d0f12] font-medium">{r.name}</td>
                    <td className="py-2.5 text-[#6b7280]">{r.useCase}</td>
                    <td className="py-2.5"><StatusChip status={r.status as "ready" | "generating" | "scheduled"} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="rounded-lg border border-[#e5e7eb] bg-[#fafafa] p-5 flex flex-col gap-3">
          <div className="text-[13px] font-semibold text-[#0d0f12] mb-1">Quick Actions</div>
          <Link href="/copilot" className="w-full">
            <Button variant="primary" className="w-full justify-start gap-2">
              <Plus className="w-3.5 h-3.5" /> Configure use case
            </Button>
          </Link>
          <Link href="/api-keys" className="w-full">
            <Button variant="secondary" className="w-full justify-start gap-2">
              <Upload className="w-3.5 h-3.5" /> Manage API keys
            </Button>
          </Link>
          <Link href="/audit-reports" className="w-full">
            <Button variant="secondary" className="w-full justify-start gap-2">
              <Download className="w-3.5 h-3.5" /> Generate report
            </Button>
          </Link>
        </div>
      </div>
    </PageShell>
  );
}
