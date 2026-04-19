import { NextRequest, NextResponse } from "next/server";
import { KMS_KEYS } from "@/lib/mock-data";

export async function POST(_req: NextRequest, { params }: { params: { id: string } }) {
  const idx = KMS_KEYS.findIndex((k) => k.id === params.id);
  if (idx === -1) return NextResponse.json({ error: "Not found" }, { status: 404 });

  KMS_KEYS[idx] = {
    ...KMS_KEYS[idx],
    lastRotated: new Date().toISOString(),
    status: "active",
  };
  return NextResponse.json(KMS_KEYS[idx]);
}
