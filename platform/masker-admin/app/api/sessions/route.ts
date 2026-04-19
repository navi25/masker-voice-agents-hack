import { NextRequest, NextResponse } from "next/server";
import { SESSIONS } from "@/lib/mock-data";

export async function GET(req: NextRequest) {
  const { searchParams } = req.nextUrl;
  const status = searchParams.get("status");
  const risk = searchParams.get("risk");
  const channel = searchParams.get("channel");
  const sq = searchParams.get("q")?.toLowerCase();
  const limit = parseInt(searchParams.get("limit") ?? "100", 10);

  let results = [...SESSIONS];
  if (status) results = results.filter((s) => s.status === status);
  if (risk) results = results.filter((s) => s.riskLevel === risk);
  if (channel) results = results.filter((s) => s.channel === channel);
  if (sq) results = results.filter((s) => s.useCase.toLowerCase().includes(sq) || s.id.toLowerCase().includes(sq));

  return NextResponse.json(results.slice(0, limit));
}
