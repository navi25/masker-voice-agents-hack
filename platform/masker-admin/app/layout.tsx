import type { Metadata, Viewport } from "next";
import { Inter, JetBrains_Mono } from "next/font/google";
import "./globals.css";
import { Sidebar } from "@/components/layout/Sidebar";
import { headers } from "next/headers";

const inter = Inter({ subsets: ["latin"], variable: "--font-inter", display: "swap" });
const jetbrainsMono = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono", display: "swap" });

export const metadata: Metadata = {
  title: "Masker — Compliance Dashboard",
  description: "Real-time privacy and compliance layer for voice and text AI",
};

export const viewport: Viewport = { width: "device-width", initialScale: 1 };

const PUBLIC_PATHS = ["/login", "/onboarding", "/auth"];

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const headersList = headers();
  const pathname = headersList.get("x-pathname") ?? "";
  const isPublic = PUBLIC_PATHS.some((p) => pathname.startsWith(p));

  return (
    <html lang="en" className={`${inter.variable} ${jetbrainsMono.variable}`}>
      <body>
        {!isPublic ? (
          <div className="flex h-screen overflow-hidden bg-white">
            <Sidebar orgName="Demo Org" orgSlug="demo-org" />
            <div className="flex flex-col flex-1 overflow-hidden">
              {children}
            </div>
          </div>
        ) : (
          <>{children}</>
        )}
      </body>
    </html>
  );
}
