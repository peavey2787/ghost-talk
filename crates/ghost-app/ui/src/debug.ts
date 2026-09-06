import { recordNativeDebugLog, setNativeDebugLogging } from "./native";

let protocolDebugEnabled = false;

export async function configureProtocolDebug(enabled: boolean): Promise<void> {
  protocolDebugEnabled = enabled;
  await setNativeDebugLogging(enabled);
}

export function isProtocolDebugEnabled(): boolean {
  return protocolDebugEnabled;
}

export function protocolDebug(
  level: "debug" | "info" | "warn" | "error",
  category: string,
  event: string,
  details: Record<string, unknown> | string = "",
): void {
  if (!protocolDebugEnabled) return;
  const encoded = typeof details === "string" ? details : safeDetails(details);
  void recordNativeDebugLog(level, category, event, encoded).catch(() => undefined);
}

function safeDetails(details: Record<string, unknown>): string {
  try {
    return JSON.stringify(details, (_key, value) => {
      if (typeof value === "string" && value.length > 512) return `${value.slice(0, 512)}…`;
      return value;
    });
  } catch {
    return "{\"detail_error\":\"unserializable debug metadata\"}";
  }
}
