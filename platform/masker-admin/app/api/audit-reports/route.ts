import { NextResponse } from "next/server";
import { AUDIT_REPORTS } from "@/lib/mock-data";

export async function GET() {
  return NextResponse.json(AUDIT_REPORTS);
}
