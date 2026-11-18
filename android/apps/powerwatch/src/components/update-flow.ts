import {
  checkUpdates,
  downloadUpdate,
  getUpdateStatus,
  installUpdate,
  errorCode,
} from "../service";
import type { UpdateResult } from "../types";

type Ports = {
  check: typeof checkUpdates;
  download: typeof downloadUpdate;
  status: typeof getUpdateStatus;
  install: typeof installUpdate;
};
export type UpdateOpenReason = "user" | "automatic" | "ready";

/** Operation lifetime belongs to the app, independently of the shared dialog. */
export class UpdateFlow {
  state: UpdateResult = {
    status: "skipped",
    version: null,
    notes: null,
    size: null,
    phase: "idle",
    progress: 0,
    packageState: "missing",
  };
  private busy = false;
  private get working(): boolean {
    return (
      this.busy ||
      [
        "checking",
        "downloading",
        "waiting",
        "verifying",
        "installing",
      ].includes(this.state.phase ?? "")
    );
  }
  accept(value: UpdateResult): void {
    const wasReady = this.state.packageState === "ready";
    this.set(value);
    if (value.phase === "ready" && !wasReady && !this.busy) this.open("ready");
  }
  private restoring: Promise<void> | null = null;
  constructor(
    private changed: () => void,
    private open: (reason: UpdateOpenReason) => void,
    private ports: Ports = {
      check: checkUpdates,
      download: downloadUpdate,
      status: getUpdateStatus,
      install: installUpdate,
    },
    private isOpen: () => boolean = () => false,
  ) {}
  private set(state: UpdateResult): void {
    this.state = state;
    this.changed();
  }
  private fail(error: unknown): void {
    const code = errorCode(error);
    const missing = [
      "update_not_downloaded",
      "update_integrity",
      "update_signature",
    ].includes(code);
    this.set({
      ...this.state,
      phase: "error",
      errorCode: code,
      ...(missing ? { packageState: "missing" as const, progress: 0 } : {}),
    });
  }
  async restore(): Promise<void> {
    if (this.busy) return;
    if (this.restoring) return this.restoring;
    this.restoring = (async () => {
      try {
        this.set(await this.ports.status());
        if (this.state.packageState === "ready") this.open("ready");
      } catch (error) {
        this.fail(error);
      } finally {
        this.restoring = null;
      }
    })();
    return this.restoring;
  }
  async check(automatic: boolean): Promise<void> {
    if (this.working || this.restoring) {
      if (!automatic) this.open("user");
      return;
    }
    if (
      automatic &&
      this.state.status === "available" &&
      (!this.state.checkedAt ||
        Date.now() - Date.parse(this.state.checkedAt) < 86_400_000)
    )
      return;
    this.busy = true;
    this.set({ ...this.state, phase: "checking", errorCode: null });
    if (!automatic) this.open("user");
    try {
      const value = await this.ports.check(automatic);
      this.set({
        ...value,
        phase:
          value.phase ?? (value.status === "available" ? "available" : "idle"),
      });
      if (automatic && value.status === "available") this.open("automatic");
    } catch (error) {
      this.fail(error);
    } finally {
      this.busy = false;
    }
  }
  async download(): Promise<void> {
    if (this.working || this.restoring || this.state.packageState === "ready") {
      this.open("user");
      return;
    }
    this.busy = true;
    this.set({ ...this.state, phase: "downloading", errorCode: null });
    this.open("user");
    try {
      this.set(await this.ports.download());
      if (this.isOpen()) {
        this.busy = false;
        await this.install();
      } else this.open("ready");
    } catch (error) {
      try {
        this.state = await this.ports.status();
      } catch {
        /* Keep the known candidate if local status is unavailable. */
      }
      this.fail(error);
    } finally {
      this.busy = false;
    }
  }
  async install(): Promise<void> {
    if (this.working || this.restoring || this.state.packageState !== "ready") {
      this.open("user");
      return;
    }
    this.busy = true;
    this.set({ ...this.state, phase: "installing", errorCode: null });
    this.open("user");
    try {
      this.set({
        ...this.state,
        phase: await this.ports.install(),
        progress: 100,
      });
    } catch (error) {
      this.fail(error);
    } finally {
      this.busy = false;
    }
  }
  phase(value: string): void {
    if (
      this.working &&
      [
        "checking",
        "downloading",
        "waiting",
        "verifying",
        "ready",
        "installing",
        "permission_required",
        "installer_opened",
      ].includes(value)
    )
      this.set({ ...this.state, phase: value as UpdateResult["phase"] });
  }
  progress(value: number): void {
    if (this.working)
      this.set({ ...this.state, progress: Math.max(0, Math.min(100, value)) });
  }
  show(): void {
    this.open("user");
  }
}
