import { NextRequest, NextResponse } from "next/server";
import { auth } from "@clerk/nextjs/server";
import { db, orgs } from "@/lib/db";
import { eq } from "drizzle-orm";

export async function POST(req: NextRequest) {
  const { userId } = await auth();
  if (!userId) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const body = await req.json() as { name: string; slug: string; framework: string };

  // Idempotent — return existing org if already created
  const [existing] = await db.select().from(orgs).where(eq(orgs.ownerId, userId)).limit(1);
  if (existing) return NextResponse.json(existing);

  const [org] = await db.insert(orgs).values({
    name: body.name,
    slug: body.slug,
    ownerId: userId,
    framework: body.framework,
  }).returning();

  return NextResponse.json(org, { status: 201 });
}
