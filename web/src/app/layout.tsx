import type { Metadata } from "next";
import type { ReactNode } from "react";

import { headers } from "next/headers";
import { connection } from "next/server";

import { StyleNonce } from "@/components/style-nonce";

import "./globals.css";

export const metadata: Metadata = {
  description: "Your media APIs, together.",
  icons: { icon: "/favicon.svg" },
  title: { default: "ArrPi", template: "%s · ArrPi" },
};

export default async function RootLayout({
  children,
}: Readonly<{ children: ReactNode }>) {
  await connection();
  const nonce = (await headers()).get("x-nonce") ?? "";

  return (
    <html lang="en" className="dark">
      <body>
        <StyleNonce nonce={nonce}>{children}</StyleNonce>
      </body>
    </html>
  );
}
