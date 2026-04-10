export interface Elements {
  form: HTMLFormElement | null;
  btn: HTMLButtonElement | null;
  romInput: HTMLInputElement | null;
  status: HTMLElement | null;
  statusArticle: HTMLElement | null;
  output: HTMLElement | null;
  progressBar: HTMLProgressElement | null;
  versionText: HTMLElement | null;
  aboutDialog: HTMLDialogElement | null;
  aboutDialogTrigger: HTMLButtonElement | null;
}

export const elements: Elements = {
  form: document.getElementById("extract-form") as HTMLFormElement | null,
  btn: document.getElementById("extract-btn") as HTMLButtonElement | null,
  romInput: document.getElementById("rom-file") as HTMLInputElement | null,
  status: document.getElementById("status") as HTMLElement | null,
  statusArticle: document.getElementById("status-article") as HTMLElement | null,
  output: document.getElementById("output") as HTMLElement | null,
  progressBar: document.getElementById("progress-bar") as HTMLProgressElement | null,
  versionText: document.getElementById("version-text") as HTMLElement | null,
  aboutDialog: document.getElementById("about-dlg") as HTMLDialogElement | null,
  aboutDialogTrigger: document.getElementById("about-dlg-trigger") as HTMLButtonElement | null,
};

export function getSelectedFormat(): string {
  return (
    (document.querySelector('input[name="format"]:checked') as HTMLInputElement)?.value ?? "json"
  );
}
