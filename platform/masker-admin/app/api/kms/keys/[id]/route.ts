import { NextRequest, NextResponse } from "next/server";
import { KMS_KEYS } from "@/lib/mock-data";

export async function PATCH(req: NextRequest, { params }: { params: { id: string } }) {
  const idx = KMS_KEYS.findIndex((k) => k.id === params.id);
  if (idx === -1) return NextResponse.json({ error: "Not found" }, { status: 404 });

  const body = await req.json() as Partial<typeof KMS_KEYS[0]>;
  KMS_KEYS[idx] = { ...KMS_KEYS[idx], ...body };
  return NextResponse.json(KMS_KEYS[idx]);
}
