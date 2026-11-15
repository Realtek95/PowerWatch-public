import { uk } from "../strings";
import { icon } from "../icons";

/** One dismissal policy for every app dialog; the form itself survives confirmation. */
export class DialogDismissal {
  private readonly confirmation = document.createElement("dialog");
  private readonly feedback = document.createElement("p");
  private outsidePointer = false;

  constructor(
    private readonly dialog: HTMLDialogElement,
    private readonly dirty: () => boolean,
    private readonly busy: () => boolean,
  ) {
    this.feedback.className = "dialog-feedback";
    this.feedback.setAttribute("role", "status");
    this.feedback.hidden = true;
    dialog.querySelector(".dialog-header")?.after(this.feedback);
    this.confirmation.className = "discard-dialog";
    this.confirmation.setAttribute("aria-labelledby", "discard-title");
    this.confirmation.innerHTML = `<div class="dialog-header"><h2 id="discard-title">${uk.unsavedTitle}</h2><button class="icon-button" data-keep-editing aria-label="${uk.continueEditing}">${icon("x")}</button></div><div class="dialog-body"><p>${uk.unsavedBody}</p><div class="dialog-actions"><button class="primary-button" data-keep-editing autofocus>${uk.continueEditing}</button><button class="text-button danger-text" data-discard>${uk.discardChanges}</button></div></div>`;
    dialog.after(this.confirmation);
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      this.requestClose();
    });
    dialog.addEventListener("pointerdown", (event) => {
      this.outsidePointer = this.outside(event);
    });
    dialog.addEventListener("pointercancel", () => {
      this.outsidePointer = false;
    });
    dialog.addEventListener("click", (event) => {
      if (this.outsidePointer && this.outside(event)) this.requestClose();
      this.outsidePointer = false;
    });
    this.confirmation.addEventListener("cancel", (event) => {
      event.preventDefault();
      this.keepEditing();
    });
    let confirmationOutside = false;
    this.confirmation.addEventListener("pointerdown", (event) => {
      confirmationOutside = this.outside(event, this.confirmation);
    });
    this.confirmation.addEventListener("pointercancel", () => {
      confirmationOutside = false;
    });
    this.confirmation.addEventListener("click", (event) => {
      if (confirmationOutside && this.outside(event, this.confirmation))
        this.keepEditing();
      confirmationOutside = false;
      const target = event.target as HTMLElement;
      if (target.closest("[data-keep-editing]")) this.keepEditing();
      if (target.closest("[data-discard]")) {
        this.confirmation.close();
        this.dialog.close();
      }
    });
  }
  private outside(event: MouseEvent, dialog = this.dialog): boolean {
    if (event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return (
      event.clientX < rect.left ||
      event.clientX > rect.right ||
      event.clientY < rect.top ||
      event.clientY > rect.bottom
    );
  }
  requestClose(): void {
    if (!this.dialog.open) return;
    if (this.confirmation.open) {
      this.keepEditing();
      return;
    }
    if (this.busy()) {
      this.feedback.textContent = uk.savingInProgress;
      this.feedback.hidden = false;
      return;
    }
    if (this.dirty()) this.confirmation.showModal();
    else this.dialog.close();
  }
  keepEditing(): void {
    this.confirmation.close();
  }
  reset(): void {
    this.keepEditing();
    this.feedback.hidden = true;
    this.outsidePointer = false;
  }
}
