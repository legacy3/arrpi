"use client";

import type { ComponentProps } from "react";

import { useMap } from "ahooks";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { NavigationGuardProvider } from "nextjs-nav-guard";
import { NuqsAdapter } from "nuqs/adapters/next/app";
import { createContext, type ReactNode, use, useEffect } from "react";
const RouteContext = createContext<ReadonlyMap<string, string>>(new Map());

export function RememberedLink(
  props: { href: string } & ComponentProps<typeof Link>,
) {
  const href = useRememberedHref(props.href);

  return <Link {...props} href={href} />;
}

export function useRememberedHref(path: string) {
  return use(RouteContext).get(path) ?? path;
}

export function ViewState({ children }: Readonly<{ children: ReactNode }>) {
  const path = usePathname();
  const search = useSearchParams().toString();
  const [routes, { set }] = useMap<string, string>();

  useEffect(() => {
    set(path, search ? `${path}?${search}` : path);
  }, [path, search, set]);

  return (
    <NavigationGuardProvider>
      <NuqsAdapter>
        <RouteContext value={routes}>{children}</RouteContext>
      </NuqsAdapter>
    </NavigationGuardProvider>
  );
}
