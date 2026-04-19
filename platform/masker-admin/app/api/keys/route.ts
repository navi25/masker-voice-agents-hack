import { NextRequest, NextResponse } from "next/server";
import { getAuthedOrg } from "@/lib/auth/getOrg";
import { db, apiKeys } from "@/lib/db";
import { eq, desc } from "drizzle-orm";
import { createHash, randomBytes } from "crypto";

export async function GET() {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const keys = await db.select().from(apiKeys)
    .where(eq(apiKeys.orgId, ctx.org.id))
    .orderBy(desc(apiKeys.createdAt));

  return NextResponse.json(keys);
}

export async function POST(req: NextRequest) {
  const ctx = await getAuthedOrg();
  if (!ctx) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const body = await req.json() as {
    label: string;
    permissions: string[];
    environment: string;
    orgId?: string;
  };

  const orgId = body.orgId ?? ctx.org.id;
  const envTag = body.environment === "production" ? "live" : "test";
  const suffix = randomBytes(4).toString("hex");
  const prefix = `msk_${envTag}_${suffix}`;
  const fullKey = `${prefix}_${randomBytes(16).toString("hex")}`;
  const keyHash = createHash("sha256").update(fullKey).digest("hex");

  const [key] = await db.insert(apiKeys).values({
    orgId,
    label: body.label,
    keyHash,
    prefix,
    permissions: body.permissions,
    environment: body.environment,
    status: "active",
  }).returning();

  return NextResponse.json({ ...key, full_key: fullKey }, { status: 201 });
}
