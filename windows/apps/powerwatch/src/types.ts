export type ScheduleState = "available" | "outage" | "switching" | "unknown";
export interface RuntimeInfo {
  startInSettings?: boolean;
  platform?: "windows" | "android" | "other";
  developerTools: boolean;
  localProjection: boolean;
  now: string;
}
export interface SourceCheck {
  httpStatus?: number;
  providerId: string;
  operatorId: string;
  operatorName: string;
  queue: string;
  checkedAt: string;
  publishedAt: string | null;
  status: "ok" | "no_schedule" | "error";
  errorCode: string | null;
}
export interface UpdateResult {
  status: "disabled" | "skipped" | "current" | "available";
  version: string | null;
  notes: string | null;
  size: number | null;
}

export interface AppInfo {
  name: string;
  author: string;
  version: string;
  buildNumber: string;
  platform: "windows" | "android" | "other";
  projectUrl: string | null;
  updatesConfigured: boolean;
  automaticUpdates: boolean;
  lastUpdateCheck: string | null;
  sourceChecks: SourceCheck[];
}
export interface ViewPreferences {
  theme: "system" | "light" | "dark";
  view: "scale" | "list";
  fontSize: "normal" | "large";
}
export interface CatalogEntry {
  id: string;
  name: string;
}
export interface SavedAddress {
  id: string;
  address: Address;
  monitoringEnabled: boolean;
  notificationsEnabled: boolean;
}
export interface SessionState {
  addresses: SavedAddress[];
  activeAddressId: string | null;
  onboardingComplete: boolean;
  checkIntervalMinutes: number;
  monitoring: MonitoringSettings;
}
export interface MonitoringSettings {
  enabled: boolean;
  workWindow: { startMinute: number; endMinute: number } | null;
}
export interface NotificationPreferences {
  enabled: boolean;
  leadMinutes: number[];
  forecastStart: boolean;
  forecastEnd: boolean;
  scheduleChanges: boolean;
  sourceStatus: boolean;
  restorationChanges: boolean;
  sourceTrouble: boolean;
  quietHours: MonitoringSettings["workWindow"];
  sound: boolean;
  vibration: boolean;
}
export interface NotificationRecord {
  id: number;
  state: string;
  updatedAt: string;
  plan: { key: string; at: string; title: string; body: string; kind: string };
}
export interface BackgroundStatus {
  notificationsAllowed?: boolean | null;
  preferences: NotificationPreferences;
  notifications: NotificationRecord[];
  polls: {
    addressKey: string;
    dueAt: string;
    failures: number;
    lastAttempt: string | null;
    lastSuccess: string | null;
    errorCode: string | null;
  }[];
  platform: "windows" | "android" | "other";
  platformSettings: { tray: boolean; autostart: boolean };
  lastError: string | null;
}
export interface AddressInput {
  oblast: string;
  settlement: string;
  street: string;
  house: string;
  building: string;
}
export interface ManualAddressSelection {
  geographic?: GeographicSelection;
  streetId: string | null;
  houseId: string | null;
  operatorId: string | null;
  queue: string | null;
  input: AddressInput;
  oblastId: string | null;
  settlementId: string | null;
  label: string;
  editingId: string | null;
}
export interface Address {
  details?: {
    geographic?: GeographicSelection | null;
    selection: {
      operator: CatalogEntry;
      queue: { operatorId: string; code: string } | null;
    } | null;
    queue: { operatorId: string; code: string } | null;
    catalog: { providerId: string; streetId: string; houseId: string } | null;
    original: AddressInput;
    normalized: AddressInput;
    oblast: CatalogEntry | null;
    settlementKatottg: string | null;
    operator: CatalogEntry | null;
    binding: { providerId: string; sourceId: string; checkedAt: string } | null;
  } | null;
  id: string;
  providerId: string;
  settlementId: string;
  streetId: string;
  settlement: string;
  street: string;
  house: string;
  label: string;
  queue: string | null;
}
export interface GeographicSelection {
  version: string;
  settlementId: string;
  districtId: string | null;
  communityId: string | null;
  streetId: string | null;
  houseId: string | null;
  postcode: string | null;
}
export interface CountryEntry extends CatalogEntry {
  number?: string | null;
  building?: string | null;
  postcode?: string | null;
}
export interface CountryQuery {
  oblastId: string;
  kind: "district" | "community" | "settlement" | "street" | "house";
  districtId?: string | null;
  communityId?: string | null;
  settlementId?: string | null;
  streetId?: string | null;
  query?: string;
}
export interface Interval {
  start: string;
  end: string;
  state: ScheduleState;
}
export interface DaySchedule {
  date: string;
  intervals: Interval[];
}
export interface DayBand {
  label: string;
  start: string;
  end: string;
  ticks: { position: number; label: string }[];
  segments: {
    intervalIndex: number;
    top: number;
    height: number;
    state: ScheduleState;
  }[];
  nowPosition: number | null;
}
export interface Dashboard {
  lastCheckError?: string;
  retryPending?: boolean;
  refreshPending?: boolean;
  now: string;
  freshness: {
    status: "current" | "stale" | "clock_mismatch";
    ageSeconds: number;
    staleAfterSeconds: number;
  };
  snapshot: {
    queueDays?: DaySchedule[] | null;
    addressCheck?: {
      catalog: { providerId: string; streetId: string; houseId: string };
      binding: { providerId: string; sourceId: string; checkedAt: string };
      evidence: {
        provider: { id: string; name: string; kind: string; website: string };
        checkedAt: string;
        publishedAt: string | null;
        validUntil: string | null;
        status: string;
      };
      event: { id: string; expectedRestoration: string | null } | null;
      queue: { operatorId: string; code: string } | null;
    } | null;
    provenance?:
      | {
          start: string;
          end: string;
          evidence: {
            provider: {
              id: string;
              name: string;
              kind: string;
              website: string;
            };
            checkedAt: string;
            publishedAt: string | null;
            validUntil: string | null;
            status: string;
          }[];
        }[]
      | null;
    providerId: string;
    isDemo: boolean;
    address: Address;
    receivedAt: string;
    publishedAt: string | null;
    sourceStatus: "unknown" | "reported_available" | "reported_unavailable";
    reason: string | null;
    expectedRestoration: string | null;
    days: DaySchedule[];
  };
  selectedDay: DaySchedule;
  bands: DayBand[];
  currentState: ScheduleState;
  nextEvent: {
    kind: "outage_start" | "outage_end";
    at: string;
    periodStart: string;
    periodEnd: string;
    periodEndKnown: boolean;
  } | null;
  nextEventUncertain: boolean;
  knownOutageSeconds: number;
  hasUncertainty: boolean;
  clockChange: boolean;
}
export interface ArchivedDay {
  provenance?: Dashboard["snapshot"]["provenance"];
  date: string;
  quality: "complete" | "partial" | "missing";
  day: DaySchedule | null;
  receivedAt: string | null;
  publishedAt: string | null;
  frozenAt: string;
  knownOutageSeconds: number;
  uncertainSeconds: number;
  outageCount: number;
  durationSeconds: number;
}
export interface ScheduleChange {
  kind:
    | "added"
    | "cancelled"
    | "moved"
    | "extended"
    | "shortened"
    | "intervals_revised"
    | "data_availability"
    | "source_status"
    | "queue"
    | "expected_restoration";
  before: Interval | null;
  after: Interval | null;
}
export interface ChangeRecord {
  id: number;
  detectedAt: string;
  detectedDate: string;
  publishedAt: string | null;
  changes: ScheduleChange[];
  before: Dashboard["snapshot"];
  after: Dashboard["snapshot"];
}
export interface ArchiveView {
  addresses: { key: string; address: Address; retired: boolean }[];
  selectedKey: string | null;
  days: ArchivedDay[];
  statistics: {
    knownOutageSeconds: number;
    uncertainSeconds: number;
    completeDays: number;
    partialDays: number;
    missingDays: number;
    averageCompleteDaySeconds: number | null;
    outageCount: number;
  };
  changes: ChangeRecord[];
  dashboard: Dashboard | null;
}
