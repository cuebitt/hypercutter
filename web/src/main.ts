import { loadPyodide, version as pyodideVersion, type PyodideInterface } from "pyodide";
import { elements, getSelectedFormat } from "./elements";
import { KNOWN_ROMS, type SymResult } from "./constants";
import { getSymFromDB, saveSymToDB } from "./database";
import { downloadFile, sha256 } from "./utils";
import extractorCode from "./assets/extractor.py?raw";

const isDev = import.meta.env.DEV;

let pyodide: PyodideInterface | null = null;
let isReady = false;

async function loadSymFile(romBuffer: ArrayBuffer): Promise<SymResult> {
  if (!elements.status) throw new Error("Missing status element");

  elements.status.textContent = "Identifying ROM...";
  const romHash = await sha256(romBuffer);
  const romInfo = KNOWN_ROMS[romHash];

  if (!romInfo) {
    throw new Error(`Unidentified ROM. SHA256: ${romHash}. Not supported.`);
  }

  elements.status.textContent = `Detected: ${romInfo.game}`;
  const cacheKey = `hypercutter_sym_${romInfo.sym}`;
  const filename = `/tmp/${romInfo.sym}`;
  elements.status.textContent = "Loading symbol file...";

  const cached = await getSymFromDB(cacheKey);
  if (cached && pyodide) {
    pyodide.FS.writeFile(filename, cached);
    return { filename, symFilename: romInfo.sym, gameName: romInfo.game };
  }

  const url = `https://cdn.jsdelivr.net/gh/pret/${romInfo.repo}@symbols/${romInfo.sym}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error("Failed to download");
  const text = await response.text();
  await saveSymToDB(cacheKey, text);
  if (pyodide) pyodide.FS.writeFile(filename, text);
  return { filename, symFilename: romInfo.sym, gameName: romInfo.game };
}

async function initPyodide(): Promise<void> {
  if (!elements.status || !elements.progressBar || !elements.versionText) return;

  elements.status.textContent = "Loading Pyodide...";
  elements.status.className = "loading";

  pyodide = await loadPyodide({
    indexURL: `https://cdn.jsdelivr.net/pyodide/v${pyodideVersion}/full/`,
  });

  (globalThis as { updateProgressBar?: (value: number) => void }).updateProgressBar = (
    value: number,
  ) => {
    elements.progressBar!.value = value;
  };

  elements.status.textContent = "Installing package...";
  if (!pyodide) return;

  try {
    await pyodide.loadPackage("micropip");
    const micropip = pyodide.pyimport("micropip");

    if (isDev) {
      await micropip.install("/dist/hypercutter-0.0.0-py3-none-any.whl");
    } else {
      await micropip.install("hypercutter");
    }

    const freeze = JSON.parse(micropip.freeze());
    const hypercutterVersion = freeze.packages.hypercutter?.version ?? "latest";
    elements.versionText.textContent = `v${hypercutterVersion}`;
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    elements.status.textContent = `Error: ${msg}`;
    elements.status.className = "error";
    return;
  }

  isReady = true;
  elements.status.textContent = "Ready";
  elements.status.className = "";
  if (elements.btn && elements.romInput) {
    elements.btn.disabled = !!elements.romInput.files?.[0];
  }
}

function runExtraction(): void {
  if (!elements.form || !elements.btn || !elements.romInput) return;

  elements.form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!isReady) return;

    const format = getSelectedFormat();
    if (elements.status && elements.output && elements.statusArticle && elements.btn) {
      elements.status.textContent = "Extracting...";
      elements.status.className = "loading";
      elements.output.style.display = "none";
      elements.statusArticle.style.display = "block";
      elements.btn.disabled = true;
    }

    try {
      const file = elements.romInput!.files?.[0];
      if (!file) throw new Error("No file selected");

      const romBuffer = await file.arrayBuffer();
      if (!pyodide) throw new Error("Pyodide not initialized");

      const importlib = pyodide.pyimport("importlib");
      const pathlib = pyodide.pyimport("pathlib");
      pathlib.Path("extractor.py").write_text(extractorCode);
      importlib.invalidate_caches();

      const { filename: symPath } = await loadSymFile(romBuffer);
      const romFilename = "/tmp/rom.gba";
      pyodide.FS.writeFile(romFilename, new Uint8Array(romBuffer));

      const extractor = pyodide.pyimport("extractor");
      const metatiles = extractor.extract_metatiles(symPath, romFilename);
      pathlib.Path("/tmp/metatiles.json").write_text(JSON.stringify(metatiles));

      if (format === "json" || format === "both") {
        downloadFile(
          new Uint8Array(pyodide.FS.readFile("/tmp/metatiles.json")),
          "metatiles.json",
          "application/json",
        );
      }

      if (format === "png" || format === "both") {
        if (elements.status && elements.progressBar) {
          elements.status.textContent = "Rendering images...";
          elements.progressBar.style.display = "block";
          elements.progressBar.value = 0;
        }

        const jsonModule = pyodide.pyimport("json");
        const metatilesData = jsonModule.loads(pathlib.Path("/tmp/metatiles.json").read_text());
        extractor.render_images(metatilesData, romFilename);

        downloadFile(
          new Uint8Array(pyodide.FS.readFile("/tmp/tilesets.zip")),
          "tilesets.zip",
          "application/zip",
        );
      }

      if (elements.status && elements.output && elements.statusArticle && elements.progressBar) {
        elements.status.textContent = "Done";
        elements.status.className = "";
        elements.output.textContent = `Extraction complete (${format} format)`;
        elements.output.style.display = "block";
        elements.progressBar.style.display = "none";
      }

      setTimeout(() => {
        elements.statusArticle!.style.display = "none";
      }, 2500);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      if (elements.status && elements.progressBar) {
        elements.status.textContent = `Error: ${msg}`;
        elements.status.className = "error";
        elements.progressBar.style.display = "none";
      }
    } finally {
      if (elements.btn) elements.btn.disabled = false;
    }
  });

  elements.romInput.addEventListener("change", () => {
    if (elements.btn && elements.romInput) {
      elements.btn.disabled = !isReady || !elements.romInput.files?.[0];
    }
  });

  elements.aboutDialogTrigger?.addEventListener("click", () => {
    elements.aboutDialog?.showModal();
  });

  elements.aboutDialog?.querySelector('button[rel="prev"]')?.addEventListener("click", () => {
    elements.aboutDialog?.close();
  });
}

async function init(): Promise<void> {
  runExtraction();
  await initPyodide();
}

void init();
