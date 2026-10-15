"use client";

import { KeyRound } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { Session } from "@/lib/session";

import { Icon } from "@/components/icon";
import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Api } from "@/lib/api";
import { openSession } from "@/lib/session";

export function Connect({
  connected,
  error: externalError,
  expired,
}: Readonly<{
  error: string;
  connected: (session: Session) => void;
  expired: () => void;
}>) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const pendingRef = useRef<Api | null>(null);
  const aliveRef = useRef(true);

  useEffect(() => {
    aliveRef.current = true;

    return () => {
      aliveRef.current = false;
      pendingRef.current?.close();
    };
  }, []);

  async function connect() {
    if (busy) {
      return;
    }

    setBusy(true);
    setError("");
    let candidate: Api | null = null;

    try {
      candidate = new Api(draft.trim(), expired);
      pendingRef.current = candidate;
      setDraft("");
      const session = await openSession(candidate);

      if (!aliveRef.current) {
        candidate.close();

        return;
      }

      pendingRef.current = null;
      connected(session);
    } catch (error_) {
      candidate?.close();

      if (aliveRef.current) {
        setError(
          error_ instanceof Error ? error_.message : "Connection failed",
        );
      }
    } finally {
      pendingRef.current = null;

      if (aliveRef.current) {
        setBusy(false);
      }
    }
  }

  return (
    <Card className="mx-auto my-8 w-full max-w-md gap-6 px-6 py-6 sm:my-14 sm:px-8 sm:py-8">
      <CardHeader className="gap-4 px-0">
        <span className="flex size-10 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary">
          <Icon icon={KeyRound} />
        </span>
        <CardTitle>
          <h2 className="text-xl font-semibold tracking-tight">
            Connect to ArrPi
          </h2>
        </CardTitle>
      </CardHeader>
      <form
        className="grid gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          void connect();
        }}
      >
        <Label htmlFor="client-key">Client key</Label>
        <Input
          id="client-key"
          type="password"
          autoComplete="one-time-code"
          required
          minLength={32}
          maxLength={128}
          placeholder="Your scoped API key"
          spellCheck={false}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button className="mt-2 w-full" disabled={busy}>
          {busy ? "Connecting…" : "Connect"}
        </Button>
      </form>
      {(error || externalError) && <Notice>{error || externalError}</Notice>}
      {busy && (
        <span className="sr-only" role="status">
          Connecting…
        </span>
      )}
      <span className="text-xs text-muted-foreground">Not saved by ArrPi.</span>
    </Card>
  );
}

export function ProxyConnect({
  connected,
  expired,
}: Readonly<{ connected: (session: Session) => void; expired: () => void }>) {
  const [error, setError] = useState("");

  useEffect(() => {
    const api = new Api(undefined, expired);
    let active = true;
    let transferred = false;

    void openSession(api)
      .then((session) => {
        if (active) {
          transferred = true;
          connected(session);
        }
      })
      .catch((error_: unknown) => {
        if (active) {
          setError(
            error_ instanceof Error ? error_.message : "Unable to sign in",
          );
        }
      });

    return () => {
      active = false;

      // The dashboard owns the API after successful connection.
      if (!transferred) {
        api.close();
      }
    };
  }, [connected, expired]);

  return (
    <Card className="mx-auto my-8 max-w-md gap-4 p-6">
      {error ? (
        <>
          <Notice>{error}</Notice>
          <Button onClick={() => location.reload()}>Retry</Button>
        </>
      ) : (
        <p role="status" className="text-sm text-muted-foreground">
          Signing in…
        </p>
      )}
    </Card>
  );
}
