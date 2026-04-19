import { NextRequest, NextResponse } from "next/server";
import { API_KEYS } from "@/lib/mock-data";

export async function PATCH(req: NextRequest, { params }: { params: { id: string } }) {
  const idx = API_KEYS.findIndex((k) => k.id === params.id);
  if (idx === -1) return NextResponse.json({ error: "Not found" }, { status: 404 });

  const body = await req.json() as Partial<typeof API_KEYS[0]>;
  API_KEYS[idx] = { ...API_KEYS[idx], ...body };
  return NextResponse.json(API_KEYS[idx]);
}
