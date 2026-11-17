import { Autocomplete } from "./autocomplete";
import { uk } from "../strings";
import {
  errorCode,
  getOblasts,
  getLocalSettlements,
  getCountryCatalog,
  getOperators,
  getAddressLookupSupport,
  getOperatorGroups,
  searchAddressCatalog,
  previewAddressGroup,
  resolveAddressGroup,
  saveManualAddress,
} from "../service";
import type {
  CatalogEntry,
  CountryEntry,
  GeographicSelection,
  SavedAddress,
  SessionState,
} from "../types";

type CatalogApi = {
  oblasts: typeof getOblasts;
  cities: typeof getLocalSettlements;
  country: typeof getCountryCatalog;
  operators: typeof getOperators;
  lookupSupport: typeof getAddressLookupSupport;
  groups: typeof getOperatorGroups;
  search: typeof searchAddressCatalog;
  previewGroup: typeof previewAddressGroup;
  resolveGroup: typeof resolveAddressGroup;
  save: typeof saveManualAddress;
};
const api: CatalogApi = {
  oblasts: getOblasts,
  cities: getLocalSettlements,
  country: getCountryCatalog,
  operators: getOperators,
  lookupSupport: getAddressLookupSupport,
  groups: getOperatorGroups,
  search: searchAddressCatalog,
  previewGroup: previewAddressGroup,
  resolveGroup: resolveAddressGroup,
  save: saveManualAddress,
};

/** Catalog IDs are optional; typing clears identity rather than fabricating it. */
export class AddressWizard {
  private readonly form = document.createElement("form");
  private readonly oblast: Autocomplete;
  private readonly city: Autocomplete;
  private readonly district: Autocomplete;
  private readonly community: Autocomplete;
  private geographicVersion: string | null = null;
  private countryHouses = new Map<string, CountryEntry>();
  private territoryRevision = 0;
  private readonly territoryDetails = document.createElement("details");
  private cityChoices: CatalogEntry[] = [];
  private readonly territoryHint = document.createElement("p");
  private readonly postcode = document.createElement("p");
  private readonly street: Autocomplete;
  private readonly house: Autocomplete;
  private readonly operator: Autocomplete;
  private readonly group: Autocomplete;
  private groupOptions: CatalogEntry[] = [];
  private groupRevision = 0;
  private matchedGroup: string | null | undefined;
  private resolvingGroup = false;
  private readonly groupNote = document.createElement("p");
  private readonly detectGroup = document.createElement("button");
  private readonly lookupNote = document.createElement("p");
  private lookupSupported: boolean | null = null;
  private supportRevision = 0;
  private touched = false;
  private lookupRevision = 0;
  private lookupTimer: ReturnType<typeof setTimeout> | undefined;
  private readonly building = document.createElement("input");
  private readonly name = document.createElement("input");
  private readonly save = document.createElement("button");
  private readonly message = document.createElement("p");
  private revision = 0;
  private busy = false;

  constructor(
    container: HTMLElement,
    private readonly initial: SavedAddress | null,
    private readonly onSaved: (state: SessionState) => void,
    private readonly catalog: CatalogApi = api,
  ) {
    this.form.className = "address-form";
    const help = document.createElement("p");
    help.id = "address-help";
    help.className = "muted";
    help.textContent = uk.chooseFromList;
    this.oblast = new Autocomplete(
      "address-oblast",
      uk.oblast,
      uk.oblastPlaceholder,
      (entry) => void this.oblastChanged(entry),
    );
    this.city = new Autocomplete(
      "address-city",
      uk.city,
      uk.cityPlaceholder,
      (entry) => {
        if (entry) {
          this.territoryHint.hidden = true;
          if (!this.district.value && !this.community.value)
            this.territoryDetails.open = false;
        }
        this.cancelLookup();
        this.street.setOptions([]);
        this.house.setOptions([]);
        this.building.value = "";
        this.clearCountryHouse();
        this.clearGroup();
        void this.refreshLookupSupport();
        this.updateSave();
      },
    );
    this.district = new Autocomplete(
      "address-district",
      uk.district,
      uk.optionalTerritory,
      () => void this.territoryChanged(true),
    );
    this.community = new Autocomplete(
      "address-community",
      uk.community,
      uk.optionalTerritory,
      () => void this.territoryChanged(false),
    );
    this.territoryDetails.className = "address-territory";
    this.territoryDetails.hidden = true;
    const territorySummary = document.createElement("summary");
    territorySummary.textContent = uk.clarifyTerritory;
    this.territoryHint.className = "muted";
    this.territoryHint.textContent = uk.territoryAmbiguous;
    this.territoryHint.hidden = true;
    this.territoryDetails.append(
      territorySummary,
      this.territoryHint,
      this.district.element,
      this.community.element,
    );
    this.postcode.hidden = true;
    this.city.input.addEventListener("input", () => {
      const query = this.city.input.value.trim().toLocaleLowerCase("uk-UA");
      const ambiguous =
        query.length >= 3 &&
        this.cityChoices.filter(
          (item) =>
            item.name.split(" · ")[0]?.trim().toLocaleLowerCase("uk-UA") ===
            query,
        ).length > 1;
      this.territoryHint.hidden = !ambiguous;
      if (ambiguous) this.territoryDetails.open = true;
    });
    this.district.setOptions([], true);
    this.community.setOptions([], true);
    this.street = new Autocomplete(
      "address-street",
      uk.street,
      uk.streetPlaceholder,
      (entry) => {
        this.house.setOptions([]);
        this.building.value = "";
        this.clearCountryHouse();
        this.clearGroup();
        this.lookup(entry ? "house" : "street");
        this.updateSave();
      },
    );
    this.house = new Autocomplete(
      "address-house",
      uk.house,
      uk.housePlaceholder,
      (entry) => {
        this.clearGroup();
        if (entry) {
          this.cancelLookup();
          const country = this.countryHouses.get(entry.id);
          if (country) {
            this.building.value = country.building ?? "";
            this.postcode.textContent = country.postcode
              ? `${uk.postcode}: ${country.postcode}`
              : "";
          }
        } else {
          this.postcode.textContent = "";
          this.lookup("house");
        }
        this.updateSave();
      },
    );
    this.operator = new Autocomplete(
      "address-operator",
      uk.gridOperator,
      uk.gridOperatorPlaceholder,
      (entry) => void this.operatorChanged(entry),
    );
    this.group = new Autocomplete(
      "address-group",
      uk.group,
      uk.groupPlaceholder,
      () => {
        this.cancelLookup();
        this.updateSave();
      },
    );
    this.group.setOptions([], true);
    this.building.addEventListener("input", () => {
      this.cancelLookup();
      this.clearGroup();
      const house =
        this.house.value && this.countryHouses.get(this.house.value.id);
      if (house) {
        this.house.input.value = house.number ?? this.house.input.value;
        this.house.setSuggestions([]);
      }
      this.clearCountryHouse();
    });
    const choiceNote = this.groupNote;
    choiceNote.className = "muted";
    choiceNote.setAttribute("role", "status");
    choiceNote.setAttribute("aria-live", "polite");
    choiceNote.textContent = uk.queueChoiceNote;
    this.detectGroup.type = "button";
    this.detectGroup.className = "text-button";
    this.detectGroup.textContent = uk.detectGroup;
    this.detectGroup.addEventListener("click", () => void this.resolveGroup());
    for (const [field, limit] of [
      [this.oblast, 120],
      [this.city, 160],
      [this.street, 200],
      [this.house, 40],
    ] as const) {
      field.input.maxLength = limit;
      field.input.required = true;
    }
    this.name.id = "address-name";
    this.name.placeholder = uk.namePlaceholder;
    this.name.maxLength = 40;
    this.name.required = true;
    this.name.value = initial?.address.label ?? "";
    this.name.addEventListener("input", () => this.updateSave());
    this.building.id = "address-building";
    this.building.maxLength = 40;
    const buildingLabel = this.label(this.building, uk.building);
    const nameLabel = this.label(this.name, uk.addressName);
    const note = document.createElement("p");
    note.className = "session-note";
    note.textContent = uk.unresolvedAddress;
    this.message.className = "form-error";
    this.message.setAttribute("role", "alert");
    this.message.hidden = true;
    this.save.type = "submit";
    this.save.className = "primary-button";
    this.save.textContent = uk.save;
    this.form.append(
      help,
      this.oblast.element,
      this.territoryDetails,
      this.city.element,
      this.operator.element,
      this.street.element,
      this.house.element,
      buildingLabel,
      this.building,
      this.postcode,
      this.group.element,
      choiceNote,
      this.detectGroup,
      this.lookupNote,
      nameLabel,
      this.name,
      note,
      this.message,
      this.save,
    );
    this.form.addEventListener("submit", (event) => {
      event.preventDefault();
      void this.submit();
    });
    this.lookupNote.className = "session-note";
    this.lookupNote.setAttribute("role", "status");
    const markDirty = () => {
      this.touched = true;
    };
    this.form.addEventListener("input", markDirty);
    this.form.addEventListener("change", markDirty);
    container.replaceChildren(this.form);
    this.updateSave();
    void this.initialize();
  }
  get hasUnsavedChanges(): boolean {
    return this.touched;
  }
  get saving(): boolean {
    return this.busy;
  }
  dispose(): void {
    this.cancelLookup();
    ++this.revision;
    ++this.groupRevision;
    ++this.territoryRevision;
    ++this.supportRevision;
  }
  private label(input: HTMLInputElement, text: string): HTMLLabelElement {
    const label = document.createElement("label");
    label.htmlFor = input.id;
    label.textContent = text;
    return label;
  }
  private async initialize(): Promise<void> {
    const revision = this.revision;
    try {
      const [items, operators] = await Promise.all([
        this.catalog.oblasts(),
        this.catalog.operators(null),
      ]);
      if (revision !== this.revision) return;
      const typed = this.oblast.input.value;
      this.oblast.setOptions(items);
      this.oblast.input.value = typed;
      this.operator.setOptions(operators);
      if (this.initial && !this.touched) {
        const details = this.initial.address.details;
        if (details?.oblast) {
          this.oblast.select(details.oblast.id);
          await this.oblastChanged(this.oblast.value);
          if (revision + 1 !== this.revision || this.touched) return;
          if (details.geographic?.version === this.geographicVersion) {
            if (details.geographic.districtId) {
              this.territoryDetails.open = true;
              this.district.select(details.geographic.districtId);
              await this.territoryChanged(true);
            }
            if (details.geographic.communityId) {
              this.territoryDetails.open = true;
              this.community.select(details.geographic.communityId);
              await this.territoryChanged(false);
            }
          }
          if (details.settlementKatottg)
            this.city.select(details.settlementKatottg);
        }
        const input = details?.original;
        if (!this.oblast.value) this.oblast.input.value = input?.oblast ?? "";
        if (!this.city.value)
          this.city.input.value =
            input?.settlement ?? this.initial.address.settlement;
        this.street.input.value = input?.street ?? this.initial.address.street;
        this.house.input.value = input?.house ?? this.initial.address.house;
        this.building.value = input?.building ?? "";
        if (details?.selection) {
          this.operator.select(details.selection.operator.id);
          await this.operatorChanged(this.operator.value);
          if (!this.touched && details.selection.queue)
            this.group.select(details.selection.queue.code);
          if (!this.touched && details.catalog) {
            this.street.setSuggestions([
              { id: details.catalog.streetId, name: this.street.input.value },
            ]);
            this.street.select(details.catalog.streetId);
            this.house.setSuggestions([
              { id: details.catalog.houseId, name: this.house.input.value },
            ]);
            this.house.select(details.catalog.houseId);
            this.applyMatchedGroup(details.queue?.code ?? null);
            if (!details.queue && details.selection.queue)
              this.group.select(details.selection.queue.code);
          }
        }
        const geo = details?.geographic;
        if (
          !this.touched &&
          geo?.version === this.geographicVersion &&
          geo?.streetId
        ) {
          this.street.setSuggestions([
            { id: geo.streetId, name: this.street.input.value },
          ]);
          this.street.select(geo.streetId);
          if (geo.houseId) {
            const house = {
              id: geo.houseId,
              name: input?.house ?? "",
              number: input?.house,
              building: input?.building,
              postcode: geo.postcode,
            };
            this.countryHouses.set(house.id, house);
            this.house.setSuggestions([house]);
            this.house.select(house.id);
            this.postcode.textContent = geo.postcode
              ? `${uk.postcode}: ${geo.postcode}`
              : "";
          }
        }
      }
    } catch (error) {
      if (revision === this.revision) this.showError(error);
    }
    this.updateSave();
  }
  private async oblastChanged(entry: CatalogEntry | null): Promise<void> {
    this.cancelLookup();
    const revision = ++this.revision;
    ++this.territoryRevision;
    this.geographicVersion = null;
    this.district.setOptions([]);
    this.community.setOptions([]);
    this.clearCountryHouse();
    this.city.setOptions([]);
    this.street.setOptions([]);
    this.house.setOptions([]);
    this.building.value = "";
    this.operator.setOptions([]);
    void this.refreshLookupSupport();
    this.groupOptions = [];
    ++this.groupRevision;
    this.clearGroup();
    this.message.hidden = true;
    this.updateSave();
    try {
      const [items, operators, districts, communities] = await Promise.all([
        entry ? this.catalog.cities(entry.id) : Promise.resolve([]),
        this.catalog.operators(entry?.id ?? null),
        entry
          ? this.catalog.country({ oblastId: entry.id, kind: "district" })
          : Promise.resolve(null),
        entry
          ? this.catalog.country({ oblastId: entry.id, kind: "community" })
          : Promise.resolve(null),
      ]);
      if (revision !== this.revision) return;
      const typed = this.city.input.value;
      this.cityChoices = items;
      this.city.setOptions(items);
      this.territoryDetails.hidden = !entry;
      this.city.input.value = typed;
      this.district.setOptions(
        districts?.items ?? [],
        !districts?.items.length,
      );
      this.community.setOptions(
        communities?.items ?? [],
        !communities?.items.length,
      );
      this.geographicVersion = districts?.version ?? null;
      const typedOperator = this.operator.input.value;
      this.operator.setOptions(operators);
      this.operator.input.value = typedOperator;
    } catch (error) {
      if (revision === this.revision) this.showError(error);
    }
  }
  private clearCountryHouse(): void {
    this.countryHouses.clear();
    this.postcode.textContent = "";
  }
  private async territoryChanged(districtChanged: boolean): Promise<void> {
    this.cancelLookup();
    const revision = ++this.territoryRevision;
    if (districtChanged) this.community.setOptions([]);
    this.city.setOptions([]);
    void this.refreshLookupSupport();
    this.street.setOptions([]);
    this.house.setOptions([]);
    this.building.value = "";
    this.clearCountryHouse();
    this.clearGroup();
    this.updateSave();
    const oblast = this.oblast.value;
    if (!oblast) return;
    const request = {
      oblastId: oblast.id,
      districtId: this.district.value?.id ?? null,
      communityId: this.community.value?.id ?? null,
    };
    try {
      const [cities, communities] = await Promise.all([
        this.catalog.country({ ...request, kind: "settlement" }),
        districtChanged
          ? this.catalog.country({ ...request, kind: "community" })
          : Promise.resolve(null),
      ]);
      if (revision !== this.territoryRevision) return;
      this.cityChoices = cities.items;
      this.city.setOptions(cities.items);
      this.geographicVersion = cities.version;
      if (communities)
        this.community.setOptions(communities.items, !communities.items.length);
    } catch (error) {
      if (revision === this.territoryRevision) this.showError(error);
    }
  }
  private clearGroup(): void {
    this.matchedGroup = undefined;
    this.groupNote.textContent = uk.queueChoiceNote;
    this.group.setOptions(
      this.groupOptions,
      !this.operator.value || this.groupOptions.length === 0,
    );
  }
  private async operatorChanged(entry: CatalogEntry | null): Promise<void> {
    this.cancelLookup();
    this.street.setSuggestions([]);
    this.house.setSuggestions([]);
    const revision = ++this.groupRevision;
    this.groupOptions = [];
    this.clearGroup();
    void this.refreshLookupSupport();
    this.updateSave();
    if (!entry) return;
    this.lookup("street");
    try {
      const groups = await this.catalog.groups(entry.id);
      if (revision !== this.groupRevision) return;
      this.groupOptions = groups;
      if (this.matchedGroup === undefined && !this.resolvingGroup)
        this.clearGroup();
    } catch (error) {
      if (revision === this.groupRevision) this.showError(error);
    }
    this.updateSave();
  }
  private async refreshLookupSupport(): Promise<void> {
    const revision = ++this.supportRevision;
    this.lookupSupported = null;
    this.updateSave();
    const operator = this.operator.value;
    const city = this.city.value;
    if (!operator || !city) return;
    try {
      const supported = await this.catalog.lookupSupport({
        operatorId: operator.id,
        settlementId: city.id,
      });
      if (revision === this.supportRevision && this.form.isConnected)
        this.lookupSupported = supported;
    } catch {
      if (revision === this.supportRevision) this.lookupSupported = false;
    }
    if (revision === this.supportRevision) this.updateSave();
  }
  private addressInput() {
    const house =
      this.house.value && this.countryHouses.get(this.house.value.id);
    return {
      oblast: this.oblast.input.value,
      settlement: this.city.value
        ? this.city.value.name.split(" · ")[0]!
        : this.city.input.value,
      street: this.street.input.value,
      house: house?.number ?? this.house.input.value,
      building: this.building.value,
    };
  }
  private applyMatchedGroup(code: string | null): void {
    this.matchedGroup = code;
    this.groupNote.textContent = code
      ? `${uk.groupMatched}: ${code}`
      : uk.groupUnknownManual;
    this.group.setOptions(
      code ? [{ id: code, name: code }] : this.groupOptions,
      code !== null || this.groupOptions.length === 0,
    );
    if (code) this.group.select(code);
    this.updateSave();
  }
  private async resolveGroup(): Promise<void> {
    const operator = this.operator.value,
      city = this.city.value;
    if (
      this.resolvingGroup ||
      !operator ||
      this.lookupSupported !== true ||
      !city ||
      !this.street.input.value.trim() ||
      !this.house.input.value.trim() ||
      this.building.value.trim()
    )
      return;
    this.cancelLookup();
    const revision = this.lookupRevision;
    const input = this.addressInput();
    this.resolvingGroup = true;
    this.touched = true;
    this.clearGroup();
    this.updateSave();
    this.groupNote.textContent = uk.groupChecking;
    try {
      const street = this.street.value,
        house = this.house.value;
      const ids = street && house && !street.id.startsWith("geo:");
      const match = ids
        ? {
            streetId: street.id,
            houseId: house.id,
            queue: await this.catalog.previewGroup({
              operatorId: operator.id,
              settlementId: city.id,
              streetId: street.id,
              houseId: house.id,
              input,
            }),
          }
        : await this.catalog.resolveGroup({
            operatorId: operator.id,
            settlementId: city.id,
            input,
          });
      if (revision !== this.lookupRevision) return;
      // Source IDs stay separate from the original text and are checked again
      // in Rust on save. A geographic row ID can never stand in for them.
      this.street.setSuggestions([{ id: match.streetId, name: input.street }]);
      this.street.select(match.streetId);
      this.house.setSuggestions([{ id: match.houseId, name: input.house }]);
      this.house.select(match.houseId);
      this.applyMatchedGroup(match.queue);
    } catch {
      if (revision === this.lookupRevision)
        this.groupNote.textContent = uk.groupLookupFailed;
    } finally {
      if (revision === this.lookupRevision) {
        this.resolvingGroup = false;
        this.updateSave();
      }
    }
  }
  private cancelLookup(): void {
    ++this.lookupRevision;
    clearTimeout(this.lookupTimer);
    this.queuedLookup = null;
    this.resolvingGroup = false;
    this.street.setLoading(false);
    this.house.setLoading(false);
  }
  private lookupRunning = false;
  private queuedLookup: (() => Promise<void>) | null = null;
  private async drainLookup(): Promise<void> {
    if (this.lookupRunning || !this.queuedLookup) return;
    const task = this.queuedLookup;
    this.queuedLookup = null;
    this.lookupRunning = true;
    try {
      await task();
    } finally {
      this.lookupRunning = false;
      void this.drainLookup();
    }
  }
  private lookup(kind: "street" | "house"): void {
    this.cancelLookup();
    const operator = this.operator.value,
      city = this.city.value,
      oblast = this.oblast.value;
    const field = kind === "street" ? this.street : this.house;
    const streetId = kind === "house" ? this.street.value?.id : null;
    if (
      !city ||
      !oblast ||
      (kind === "street" && field.input.value.trim().length < 2) ||
      (kind === "house" && !streetId)
    )
      return;
    const revision = this.lookupRevision;
    const query = field.input.value;
    field.setLoading(true);
    this.lookupTimer = setTimeout(() => {
      if (!this.form.isConnected) return;
      this.queuedLookup = async () => {
        if (revision !== this.lookupRevision || !this.form.isConnected) return;
        let localError: unknown;
        try {
          // Offline suggestions are useful immediately. Only an empty local
          // result needs the operator's network catalog.
          let local: Awaited<ReturnType<CatalogApi["country"]>> | null = null;
          if (kind === "street" || streetId?.startsWith("geo:")) {
            try {
              local = await this.catalog.country({
                oblastId: oblast.id,
                settlementId: city.id,
                kind,
                streetId,
                query,
              });
            } catch (error) {
              localError = error;
            }
          }
          if (revision !== this.lookupRevision) return;
          if (local) this.geographicVersion = local.version;
          let items: CatalogEntry[] = local?.items ?? [];
          if (!items.length && operator && !streetId?.startsWith("geo:"))
            items = await this.catalog.search({
              operatorId: operator.id,
              settlementId: city.id,
              streetId: streetId ?? null,
              query,
            });
          if (revision !== this.lookupRevision) return;
          if (kind === "house") {
            this.countryHouses.clear();
            for (const item of local?.items ?? [])
              this.countryHouses.set(item.id, item);
          }
          field.setSuggestions(items);
          if (!items.length && localError) this.showError(localError);
        } catch (error) {
          if (revision === this.lookupRevision) this.showError(error);
        } finally {
          if (revision === this.lookupRevision) field.setLoading(false);
        }
      };
      void this.drainLookup();
    }, 350);
  }

  private geographic(): GeographicSelection | undefined {
    if (!this.city.value || !this.geographicVersion) return undefined;
    const street = this.street.value?.id.startsWith("geo:")
      ? this.street.value.id
      : null;
    const house =
      street && this.house.value?.id.startsWith("geo:")
        ? this.house.value.id
        : null;
    return {
      version: this.geographicVersion,
      settlementId: this.city.value.id,
      districtId: this.district.value?.id ?? null,
      communityId: this.community.value?.id ?? null,
      streetId: street,
      houseId: house,
      postcode: house
        ? (this.countryHouses.get(house)?.postcode ?? null)
        : null,
    };
  }
  private valid(): boolean {
    return (
      (!this.operator.input.value.trim() || !!this.operator.value) &&
      (!this.district.input.value.trim() || !!this.district.value) &&
      (!this.community.input.value.trim() || !!this.community.value) &&
      (!this.group.input.value.trim() || !!this.group.value) &&
      [
        this.oblast.input,
        this.city.input,
        this.street.input,
        this.house.input,
        this.name,
      ].every(
        (input) =>
          input.value.trim().length > 0 &&
          [...input.value].length <= input.maxLength,
      )
    );
  }
  private updateSave(): void {
    this.postcode.hidden = !this.postcode.textContent;
    this.detectGroup.textContent = this.resolvingGroup
      ? uk.groupChecking
      : uk.detectGroup;
    this.detectGroup.classList.toggle("is-pending", this.resolvingGroup);
    this.detectGroup.setAttribute("aria-busy", String(this.resolvingGroup));
    this.save.disabled = this.busy || this.resolvingGroup || !this.valid();
    this.detectGroup.disabled =
      this.busy ||
      this.resolvingGroup ||
      !this.operator.value ||
      this.lookupSupported !== true ||
      !this.city.value ||
      !this.street.input.value.trim() ||
      !this.house.input.value.trim() ||
      !!this.building.value.trim();
    this.detectGroup.hidden =
      !this.operator.value || this.lookupSupported === false;
    this.lookupNote.textContent = !this.operator.value
      ? uk.lookupChooseOperator
      : !this.city.value
        ? uk.lookupChooseSettlement
        : this.lookupSupported === null
          ? uk.lookupCheckingSupport
          : !this.lookupSupported
            ? uk.lookupUnsupported
            : this.building.value.trim()
              ? uk.lookupBuildingUnsupported
              : !this.street.input.value.trim() ||
                  !this.house.input.value.trim()
                ? uk.lookupCompleteAddress
                : "";
    this.lookupNote.hidden = !this.lookupNote.textContent;
  }
  private showError(error: unknown): void {
    const code = errorCode(error);
    this.message.textContent =
      code in uk.errors
        ? uk.errors[code as keyof typeof uk.errors]
        : uk.errors.internal_error;
    this.message.hidden = false;
  }
  private async submit(): Promise<void> {
    if (this.busy || this.resolvingGroup || !this.valid()) return;
    this.busy = true;
    this.form.inert = true;
    this.save.textContent = uk.saving;
    this.message.hidden = true;
    try {
      const state = await this.catalog.save({
        ...(this.geographic() ? { geographic: this.geographic() } : {}),
        streetId:
          !this.street.value?.id.startsWith("geo:") &&
          !this.building.value.trim() &&
          this.house.value
            ? (this.street.value?.id ?? null)
            : null,
        houseId:
          !this.house.value?.id.startsWith("geo:") &&
          !this.building.value.trim() &&
          this.street.value
            ? (this.house.value?.id ?? null)
            : null,
        operatorId: this.operator.value?.id ?? null,
        queue: this.matchedGroup ? null : (this.group.value?.id ?? null),
        input: this.addressInput(),
        oblastId: this.oblast.value?.id ?? null,
        settlementId: this.city.value?.id ?? null,
        label: this.name.value.trim(),
        editingId: this.initial?.id ?? null,
      });
      this.touched = false;
      this.onSaved(state);
    } catch (error) {
      this.showError(error);
    } finally {
      this.busy = false;
      this.form.inert = false;
      this.save.textContent = uk.save;
      this.updateSave();
    }
  }
}
