import { NextRequest, NextResponse } from "next/server";
import { randomBytes } from "crypto";
import { API_KEYS } from "@/lib/mock-data";

export async function POST(_req: NextRequest, { params }: { params: { id: string } }) {
  const idx = API_KEYS.findIndex((k) => k.id === params.id);
  if (idx === -1) return NextResponse.json({ error: "Not found" }, { status: 404 });

  const existing = API_KEYS[idx];
  const envTag = existing.environment === "production" ? "live" : "test";
  const suffix = randomBytes(4).toString("hex");
  const prefix = `msk_${envTag}_${suffix}`;
  const fullKey = `${prefix}_${randomBytes(16).toString("hex")}`;

  API_KEYS[idx] = { ...existing, prefix, status: "active" };
  return NextResponse.json({ ...API_KEYS[idx], fullKey });
}
