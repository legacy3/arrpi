import { format, formatDistanceToNowStrict, isValid, parseISO } from "date-fns";
import {
  Activity,
  Box,
  BrushCleaning,
  Captions,
  Download,
  Film,
  Inbox,
  Recycle,
  Tv,
} from "lucide-react";

export const apps = {
  bazarr: { icon: Captions, label: "Bazarr" },
  cleanuparr: { icon: BrushCleaning, label: "Cleanuparr" },
  crosswatch: { icon: Activity, label: "CrossWatch" },
  maintainerr: { icon: Recycle, label: "Maintainerr" },
  plex: { icon: Film, label: "Plex" },
  radarr: { icon: Film, label: "Radarr" },
  sabnzbd: { icon: Download, label: "SABnzbd" },
  seerr: { icon: Inbox, label: "Seerr" },
  sonarr: { icon: Tv, label: "Sonarr" },
} as const;

export function appInfo(kind: string) {
  return Object.hasOwn(apps, kind)
    ? apps[kind as keyof typeof apps]
    : { icon: Box, label: kind };
}

const messages: Record<string, string> = {
  capacity_exceeded: "ArrPi is busy. Try again shortly.",
  forbidden: "This account does not have permission for this action.",
  invalid_arguments: "Check the required fields and their values.",
  invalid_upstream_response: "The service returned an unexpected response.",
  maintenance_disabled_unverified_version:
    "This app version has not been verified for maintenance commands.",
  not_found: "This item or operation is no longer available.",
  permission_denied: "This account does not have permission for this action.",
  search_too_large:
    "This search exceeds the configured limit. Select fewer episodes or review MCP limits.",
  timeout: "The service took too long to respond. Try again shortly.",
  unauthorized: "Sign in again to continue.",
  unknown_outcome:
    "The service may have accepted this action. Check its progress before trying again.",
  unsafe_search_profile:
    "This profile allows 4K. Choose a non-4K profile in the library app before searching.",
  upstream_invalid_response: "The service returned an unexpected response.",
  upstream_rejected:
    "The service rejected this request. Check the supplied values.",
  upstream_timeout: "The service took too long to respond. Try again shortly.",
  upstream_unauthorized:
    "The service rejected its API key. Check the connection in Settings.",
  upstream_unavailable:
    "The service could not be reached. Check its connection or try again.",
  writes_disabled_unverified_version:
    "This service version has not been verified for requests. Reads remain available.",
};

export function errorText(code: string) {
  return messages[code] ?? code.replaceAll("_", " ");
}

export function responseError(message: string, data: unknown) {
  if (
    data &&
    typeof data === "object" &&
    "code" in data &&
    typeof data.code === "string" &&
    Object.hasOwn(messages, data.code)
  ) {
    return messages[data.code] ?? errorText(message);
  }

  return errorText(message);
}

const outcomes: Record<string, string> = {
  already_available: "Already available",
  already_requested: "Already requested",
  approved: "Approved",
  available: "Available",
  canceled: "Cancelled",
  cancelled: "Cancelled",
  capacity_exceeded: "Busy",
  closed: "Closed",
  completed: "Completed",
  failed: "Failed",
  forbidden: "Access denied",
  invalid_arguments: "Invalid input",
  maintenance_disabled_unverified_version: "Version unverified",
  not_found: "Not found",
  nothing_to_search: "Nothing to search",
  ok: "Succeeded",
  open: "Open",
  pending: "Awaiting approval",
  permission_denied: "Access denied",
  queue_timeout_retry_safe: "Queue timed out",
  queued: "Accepted",
  resolved: "Resolved",
  running: "Running",
  search_too_large: "Search too large",
  started: "Running",
  unauthorized: "Sign in required",
  unknown_outcome: "Check progress",
  unsafe_search_profile: "Unsafe profile",
  upstream_rate_limited: "Rate limited",
  upstream_rejected: "Request rejected",
  upstream_timeout: "Timed out",
  upstream_unauthorized: "Connection rejected",
  upstream_unavailable: "Service unavailable",
  writes_disabled_unverified_version: "Version unverified",
};

export function fullTime(value: string) {
  const date = parseISO(value);

  return isValid(date) ? format(date, "PPpp OOOO") : "Unknown time";
}

export function outcomeLabel(value: string) {
  return outcomes[value] ?? value.replaceAll("_", " ");
}

export function relativeTime(value: string) {
  const date = parseISO(value);

  return isValid(date)
    ? formatDistanceToNowStrict(date, { addSuffix: true })
    : "Never";
}

export function shortTime(value: string) {
  const date = parseISO(value);

  return isValid(date) ? format(date, "d MMM, HH:mm") : "Unknown time";
}
export const endpointLabels: Record<string, string> = {
  calendar: "Release calendar",
  health: "Health issues",
  movie: "Movies",
  qualityprofile: "Quality profiles",
  queue: "Download queue",
  series: "TV shows",
};
