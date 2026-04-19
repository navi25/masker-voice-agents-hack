import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, kmsKeys } from "@/lib/db";
import { eq, desc } from "drizzle-orm";

export async function GET() {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const keys = await db.select().from(kmsKeys)
    .where(eq(kmsKeys.orgId, ctx.org.id))
    .orderBy(desc(kmsKeys.createdAt));

  return NextResponse.json(keys);
}

export async function POST(req: NextRequest) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const body = await req.json() as {
    alias: string;
    scope?: string;
    region?: string;
    rotationCadence?: string;
    provider?: string;
  };

  const [key] = await db.insert(kmsKeys).values({
    orgId: ctx.org.id,
    alias: body.alias,
    scope: body.scope ?? "All sessions",
    region: body.region ?? "us-east-1",
    rotationCadence: body.rotationCadence ?? "90 days",
    status: "active",
    provider: (body.provider ?? "masker") as "masker" | "byok",
  }).returning();

  return NextResponse.json(key, { status: 201 });
}
