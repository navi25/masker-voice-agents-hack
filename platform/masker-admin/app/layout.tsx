import type { Metadata, Viewport } from "next";
import { Inter, JetBrains_Mono } from "next/font/google";
import "./globals.css";
import { ClerkProvider } from "@clerk/nextjs";
import { Sidebar } from "@/components/layout/Sidebar";
import { auth, currentUser } from "@clerk/nextjs/server";
import { db } from "@/lib/db";
import { orgs } from "@/lib/db/schema";
import { eq } from "drizzle-orm";

const inter = Inter({ subsets: ["latin"], variable: "--font-inter", display: "swap" });
const jetbrainsMono = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono", display: "swap" });

export const metadata: Metadata = {
  title: "Masker — Compliance Dashboard",
  description: "Real-time privacy and compliance layer for voice and text AI",
};

export const viewport: Viewport = { width: "device-width", initialScale: 1 };

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  let orgName: string | null = null;
  let orgSlug: string | null = null;
  let userEmail: string | null = null;

  try {
    const { userId } = await auth();
    if (userId) {
      const user = await currentUser();
      userEmail = user?.emailAddresses?.[0]?.emailAddress ?? null;
      const [org] = await db.select({ name: orgs.name, slug: orgs.slug })
        .from(orgs)
        .where(eq(orgs.ownerId, userId))
        .limit(1);
      orgName = org?.name ?? null;
      orgSlug = org?.slug ?? null;
    }
  } catch {
    // Not authenticated or DB unavailable — public page
  }

  const showShell = orgName !== null;

  return (
    <ClerkProvider>
      <html lang="en" className={`${inter.variable} ${jetbrainsMono.variable}`}>
        <body>
          {showShell ? (
            <div className="flex h-screen overflow-hidden bg-white">
              <Sidebar orgName={orgName!} orgSlug={orgSlug!} userEmail={userEmail} />
              <div className="flex flex-col flex-1 overflow-hidden">
                {children}
              </div>
            </div>
          ) : (
            <>{children}</>
          )}
        </body>
      </html>
    </ClerkProvider>
  );
}
