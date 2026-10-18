import type { Metadata } from "next";
import type { ReactNode } from "react";

import { headers } from "next/headers";
import { connection } from "next/server";

import { StyleNonce } from "@/components/style-nonce";
import { TooltipProvider } from "@/components/ui/tooltip";

import "./globals.css";

export const metadata: Metadata = {
  description: "Your media APIs, together.",
  icons: {
    apple: {
      sizes: "180x180",
      type: "image/png",
      url: "/apple-touch-icon.png",
    },
    icon: [
      { sizes: "32x32", type: "image/png", url: "/favicon-32.png" },
      { sizes: "64x64", type: "image/png", url: "/favicon-64.png" },
    ],
  },
  title: { default: "ArrPi", template: "%s | ArrPi" },
};

export default async function RootLayout({
  children,
}: Readonly<{ children: ReactNode }>) {
  await connection();
  const nonce = (await headers()).get("x-nonce") ?? "";

  return (
    <html lang="en" className="dark">
      <body>
        <StyleNonce nonce={nonce}>
          <TooltipProvider delayDuration={300}>{children}</TooltipProvider>
        </StyleNonce>
      </body>
    </html>
  );
}
