import { NextRequest, NextResponse } from "next/server";
import { randomBytes } from "crypto";
import { API_KEYS } from "@/lib/mock-data";
import type { ApiKey } from "@/lib/mock-data";

export async function GET() {
  return NextResponse.json(API_KEYS);
}

export async function POST(req: NextRequest) {
  const body = await req.json() as { label: string; permissions: string[]; environment: string };

  const envTag = body.environment === "production" ? "live" : "test";
  const suffix = randomBytes(4).toString("hex");
  const prefix = `msk_${envTag}_${suffix}`;
  const fullKey = `${prefix}_${randomBytes(16).toString("hex")}`;

  const newKey: ApiKey = {
    id: `key-${Date.now()}`,
    label: body.label,
    prefix,
    permissions: body.permissions,
    environment: body.environment,
    status: "active",
    lastUsed: "Never",
    createdAt: new Date().toISOString(),
  };

  API_KEYS.unshift(newKey);
  return NextResponse.json({ ...newKey, fullKey }, { status: 201 });
}
