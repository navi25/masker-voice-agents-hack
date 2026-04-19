"use client";

import { useRouter } from "next/navigation";

export function LoginClient() {
  const router = useRouter();

  return (
    <div className="min-h-screen bg-[#f8fafc] flex">
      {/* Left — branding */}
      <div className="hidden lg:flex flex-col justify-between w-[480px] bg-[#0d0f12] p-12 shrink-0">
        <div>
          <div className="flex items-center gap-2.5 mb-16">
            <span className="text-2xl">🛡️</span>
            <span className="text-white font-semibold text-lg tracking-tight">Masker</span>
          </div>
          <h1 className="text-[32px] font-semibold text-white leading-tight mb-4">
            Programmable privacy<br />for voice agents
          </h1>
          <p className="text-[15px] text-[#9ca3af] leading-relaxed">
            Drop-in HIPAA, GDPR, and PCI compliance. Detect PII, mask in real-time, and ship full audit trails — in one SDK call.
          </p>
        </div>

        <div className="space-y-4">
          {[
            { label: "Entity detection", value: "SSN, DOB, CC, MRN, 40+ types" },
            { label: "Masking modes", value: "mask · redact · tokenize · block" },
            { label: "Audit trail", value: "Immutable, per-session, exportable" },
            { label: "Frameworks", value: "HIPAA · GDPR · PCI · Custom" },
          ].map(({ label, value }) => (
            <div key={label} className="flex items-center justify-between text-[13px]">
              <span className="text-[#6b7280]">{label}</span>
              <span className="text-[#d1d5db] font-mono">{value}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Right — enter */}
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="w-full max-w-sm">
          <div className="flex items-center gap-2 mb-10 lg:hidden">
            <span className="text-2xl">🛡️</span>
            <span className="font-semibold text-lg text-[#0d0f12]">Masker</span>
          </div>

          <h2 className="text-[22px] font-semibold text-[#0d0f12] mb-1">Get started</h2>
          <p className="text-[14px] text-[#6b7280] mb-8">
            Open the dashboard to manage your privacy policies and API keys.
          </p>

          <button
            onClick={() => router.push("/overview")}
            className="w-full flex items-center justify-center gap-3 px-4 py-2.5 rounded-lg bg-[#0d0f12] text-white text-[14px] font-medium hover:bg-[#1f2937] transition-colors"
          >
            Enter Dashboard
          </button>
        </div>
      </div>
    </div>
  );
}
