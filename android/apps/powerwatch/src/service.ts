import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  ManualAddressSelection,
  AddressInput,
  CatalogEntry,
  Dashboard,
  SessionState,
  RuntimeInfo,
  MonitoringSettings,
  ArchiveView,
  ViewPreferences,
  BackgroundStatus,
  NotificationPreferences,
} from "./types";

const readOnly = <T>(): Promise<T> => Promise.reject({ code: "app_required" });
export function getAppInfo(): Promise<import("./types").AppInfo> {
  return isTauri() ? invoke("get_app_info") : readOnly();
}
export function setAutomaticUpdates(enabled: boolean): Promise<void> {
  return isTauri() ? invoke("set_automatic_updates", { enabled }) : readOnly();
}
export function openAppLink(id: string): Promise<void> {
  return isTauri() ? invoke("open_app_link", { id }) : readOnly();
}
export function checkUpdates(
  automatic = false,
): Promise<import("./types").UpdateResult> {
  return isTauri() ? invoke("check_updates", { automatic }) : readOnly();
}
export function installUpdate(): Promise<
  "permission_required" | "installer_opened"
> {
  return isTauri() ? invoke("install_update") : readOnly();
}
export function getViewPreferences(): Promise<ViewPreferences | null> {
  return isTauri() ? invoke("get_view_preferences") : Promise.resolve(null);
}
export function setViewPreferences(
  preferences: ViewPreferences,
): Promise<void> {
  return isTauri()
    ? invoke("set_view_preferences", { preferences })
    : Promise.resolve();
}

export function getRuntimeInfo(): Promise<RuntimeInfo> {
  return isTauri()
    ? invoke("get_runtime_info")
    : Promise.resolve({
        developerTools: false,
        localProjection: false,
        now: new Date().toISOString(),
      });
}

/** Recalculate the retained snapshot in Rust without fetching the provider. */
export function projectDashboard(
  addressId: string,
  dayOffset: number,
): Promise<Dashboard> {
  return isTauri()
    ? invoke("project_dashboard", { addressId, dayOffset })
    : readOnly();
}

export function getSession(): Promise<SessionState> {
  if (isTauri()) return invoke("get_session");
  return Promise.resolve({
    addresses: [],
    activeAddressId: null,
    onboardingComplete: false,
    checkIntervalMinutes: 30,
    monitoring: { enabled: false, workWindow: null },
  });
}
export function getBackgroundStatus(): Promise<BackgroundStatus> {
  return isTauri() ? invoke("get_background_status") : readOnly();
}
export function openBackgroundSettings(): Promise<void> {
  return isTauri() ? invoke("open_background_settings") : readOnly();
}
export function openExactReminderSettings(): Promise<void> {
  return isTauri() ? invoke("open_exact_reminder_settings") : readOnly();
}
export function clearNotificationJournal(id: number | null): Promise<void> {
  return isTauri() ? invoke("clear_notification_journal", { id }) : readOnly();
}
export function setNotificationPreferences(
  preferences: NotificationPreferences,
): Promise<void> {
  return isTauri()
    ? invoke("set_notification_preferences", { preferences })
    : readOnly();
}
export function setAddressOptions(
  id: string,
  monitoring: boolean,
  notifications: boolean,
): Promise<SessionState> {
  return isTauri()
    ? invoke("set_address_options", { id, monitoring, notifications })
    : readOnly();
}
export function setPlatformSettings(
  settings: BackgroundStatus["platformSettings"],
): Promise<void> {
  return isTauri() ? invoke("set_platform_settings", { settings }) : readOnly();
}
export function testNotification(): Promise<void> {
  return isTauri() ? invoke("test_notification") : readOnly();
}
export function getOblasts(): Promise<CatalogEntry[]> {
  return isTauri() ? invoke("get_oblasts") : readOnly();
}
export function getCountryCatalog(
  request: import("./types").CountryQuery,
): Promise<{ version: string; items: import("./types").CountryEntry[] }> {
  return isTauri()
    ? invoke("get_country_catalog", { request: { query: "", ...request } })
    : readOnly();
}
export function getLocalSettlements(oblastId: string): Promise<CatalogEntry[]> {
  return isTauri() ? invoke("get_local_settlements", { oblastId }) : readOnly();
}
export function saveManualAddress(
  selection: ManualAddressSelection,
): Promise<SessionState> {
  return isTauri() ? invoke("save_manual_address", { selection }) : readOnly();
}
export function getOperators(oblastId: string | null): Promise<CatalogEntry[]> {
  return isTauri() ? invoke("get_operators", { oblastId }) : readOnly();
}
export function getAddressLookupSupport(request: {
  operatorId: string;
  settlementId: string;
}): Promise<boolean> {
  return isTauri()
    ? invoke("get_address_lookup_support", request)
    : Promise.resolve(false);
}
export function getOperatorGroups(operatorId: string): Promise<CatalogEntry[]> {
  return isTauri() ? invoke("get_operator_groups", { operatorId }) : readOnly();
}
export function searchAddressCatalog(request: {
  operatorId: string;
  settlementId: string;
  streetId: string | null;
  query: string;
}): Promise<CatalogEntry[]> {
  return isTauri() ? invoke("search_address_catalog", { request }) : readOnly();
}
export function previewAddressGroup(request: {
  operatorId: string;
  settlementId: string;
  streetId: string;
  houseId: string;
  input: AddressInput;
}): Promise<string | null> {
  return isTauri() ? invoke("preview_address_group", { request }) : readOnly();
}
export function resolveAddressGroup(request: {
  operatorId: string;
  settlementId: string;
  input: AddressInput;
}): Promise<{ streetId: string; houseId: string; queue: string | null }> {
  return isTauri() ? invoke("resolve_address_group", { request }) : readOnly();
}
export function activateAddress(id: string): Promise<SessionState> {
  return isTauri() ? invoke("activate_address", { id }) : readOnly();
}
export function deleteAddress(id: string): Promise<SessionState> {
  return isTauri() ? invoke("delete_address", { id }) : readOnly();
}
export function getArchive(
  key: string | null = null,
  date: string | null = null,
): Promise<ArchiveView> {
  return isTauri() ? invoke("get_archive", { key, date }) : readOnly();
}
export function completeOnboarding(interval: number): Promise<SessionState> {
  return isTauri() ? invoke("complete_onboarding", { interval }) : readOnly();
}

export function setMonitoring(
  interval: number,
  monitoring: MonitoringSettings,
): Promise<SessionState> {
  return isTauri()
    ? invoke("set_monitoring", { interval, monitoring })
    : readOnly();
}

export function getDashboard(
  addressId: string,
  dayOffset: number,
): Promise<Dashboard> {
  return isTauri()
    ? invoke("get_dashboard", { addressId, dayOffset })
    : readOnly();
}
export function errorCode(error: unknown): string {
  return typeof error === "object" && error !== null && "code" in error
    ? String(error.code)
    : "internal_error";
}
