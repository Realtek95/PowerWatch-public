import { uk } from "../strings";
import { escapeHtml as html } from "../format";
import { icon } from "../icons";
import type { RuntimeInfo } from "../types";

interface Question {
  question: string;
  answer: readonly string[];
  steps?: readonly string[];
  after?: string;
}
/** Bundled, offline content. Platform help never initiates a schedule request. */
export function createHelpPage(platform: RuntimeInfo["platform"]): HTMLElement {
  const element = document.createElement("section");
  element.id = "help-page";
  element.className = "help-page";
  element.dataset.platform = platform ?? "other";
  const platformQuestions =
    platform === "android"
      ? uk.helpAndroid
      : platform === "windows"
        ? uk.helpWindows
        : [];
  const questions: readonly Question[] = [
    ...platformQuestions,
    ...uk.helpCommon,
  ];
  const setup =
    platform === "android"
      ? [uk.helpAndroidSetup, uk.helpAndroidSetupBody]
      : platform === "windows"
        ? [uk.helpWindowsSetup, uk.helpWindowsSetupBody]
        : null;
  element.innerHTML = `<button class="text-button help-back" data-action="help-back">${icon("chevron")}${uk.helpBack}</button><h2>${uk.helpTitle}</h2>
    ${setup ? `<aside class="card help-setup"><h3>${setup[0]}</h3><p>${setup[1]}</p>${platform === "android" ? `<button class="secondary-button" data-action="help-background-settings">${uk.androidBackgroundSettings}</button>` : ""}</aside>` : ""}
    <label class="help-search" for="help-search">${uk.helpSearch}<input id="help-search" type="search" autocomplete="off" aria-controls="help-questions" /></label>
    <div id="help-questions">${questions.map((item) => `<details class="help-question"><summary>${html(item.question)}</summary><div>${item.answer.map((text) => `<p>${html(text)}</p>`).join("")}${item.steps ? `<ol>${item.steps.map((text) => `<li>${html(text)}</li>`).join("")}</ol>` : ""}${item.after ? `<p>${html(item.after)}</p>` : ""}</div></details>`).join("")}</div>
    <p class="muted" data-help-empty role="status" hidden>${uk.helpEmpty}</p>`;
  const search = element.querySelector<HTMLInputElement>("#help-search")!;
  const key = (value: string) =>
    value.toLocaleLowerCase("uk-UA").replace(/[’ʼ`]/g, "'").trim();
  search.addEventListener("input", () => {
    const query = key(search.value);
    let found = false;
    element
      .querySelectorAll<HTMLDetailsElement>(".help-question")
      .forEach((item) => {
        item.hidden = !key(item.textContent ?? "").includes(query);
        if (!item.hidden) found = true;
      });
    element.querySelector<HTMLElement>("[data-help-empty]")!.hidden = found;
  });
  return element;
}
