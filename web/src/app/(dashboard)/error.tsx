"use client";

import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";

export default function PageError({ reset }: Readonly<{ reset: () => void }>) {
  return (
    <div className="space-y-3">
      <Notice>Could not open this page</Notice>
      <Button variant="outline" onClick={reset}>
        Retry
      </Button>
    </div>
  );
}
