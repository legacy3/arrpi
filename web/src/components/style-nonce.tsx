"use client";

import { setNonce } from "get-nonce";
import { type ReactNode, useEffect } from "react";

export function StyleNonce({
  children,
  nonce,
}: Readonly<{ children: ReactNode; nonce: string }>) {
  useEffect(() => {
    // Radix's scroll lock inserts styles through react-style-singleton.
    setNonce(nonce);
  }, [nonce]);

  return children;
}
