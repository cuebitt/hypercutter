import { loadPyodide, version as pyodideVersion, type PyodideInterface } from "pyodide";
import pycode from "./extractor.py?raw";

interface RomInfo {
  sym: string;
  repo: string;
  game: string;
}

interface Elements {
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

const elements: Elements = {
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

let pyodide: PyodideInterface | null = null;
let isReady = false;

const DB_NAME = "hypercutter_symdb";
const DB_VERSION = 1;
const STORE_NAME = "symfiles";

let db: IDBDatabase | null = null;

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (db) {
      resolve(db);
      return;
    }
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      db = request.result;
      resolve(db);
    };
    request.onupgradeneeded = (event) => {
      const database = (event.target as IDBOpenDBRequest).result;
      if (!database.objectStoreNames.contains(STORE_NAME)) {
        database.createObjectStore(STORE_NAME, { keyPath: "key" });
      }
    };
  });
}

async function getSymFromDB(key: string): Promise<string | null> {
  const database = await openDB();
  return new Promise((resolve, reject) => {
    const tx = database.transaction(STORE_NAME, "readonly");
    const store = tx.objectStore(STORE_NAME);
    const request = store.get(key);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result?.data ?? null);
  });
}

async function saveSymToDB(key: string, data: string): Promise<void> {
  const database = await openDB();
  return new Promise((resolve, reject) => {
    const tx = database.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    const request = store.put({ key, data });
    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve();
  });
}

function downloadFile(data: Uint8Array, filename: string, mimeType: string): void {
  const blob = new Blob([new Uint8Array(data)], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

async function sha256(buffer: ArrayBuffer): Promise<string> {
  const hashBuffer = await crypto.subtle.digest("SHA-256", buffer);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
}

const knownRoms: Record<string, RomInfo> = {
  "53d591215de2cab847d14fbcf8c516f0128cfa8556f1236065e0535aa5936d4e": {
    sym: "pokeruby.sym",
    repo: "pokeruby",
    game: "Pokemon Ruby",
  },
  "0d80909998a901c7edef5942068585bc855a85aec7e083aa6aeff84a5b2f8ec0": {
    sym: "pokeruby_rev1.sym",
    repo: "pokeruby",
    game: "Pokemon Ruby v1.1",
  },
  "0fdd36e92b75bed65d09df4635ab0b707b288c2bf1dc4c6e7a4a4f0eebe9d64c": {
    sym: "pokeruby_rev2.sym",
    repo: "pokeruby",
    game: "Pokemon Ruby v1.2",
  },
  c36c1b899503e8823ee7eb607eea583adcef7ea92ff804838b193c227f2c6657: {
    sym: "pokesapphire.sym",
    repo: "pokeruby",
    game: "Pokemon Sapphire",
  },
  "2f680a43e5c57aede4cb3b2cb04f7e15079efc122c88edaacfd6026db6e920ac": {
    sym: "pokesapphire_rev1.sym",
    repo: "pokeruby",
    game: "Pokemon Sapphire v1.1",
  },
  "02ca41513580a8b780989dee428df747b52a0b1a55bec617886b4059eb1152fb": {
    sym: "pokesapphire_rev2.sym",
    repo: "pokeruby",
    game: "Pokemon Sapphire v1.2",
  },
  a9dec84dfe7f62ab2220bafaef7479da0929d066ece16a6885f6226db19085af: {
    sym: "pokeemerald.sym",
    repo: "pokeemerald",
    game: "Pokemon Emerald",
  },
  "3d0c79f1627022e18765766f6cb5ea067f6b5bf7dca115552189ad65a5c3a8ac": {
    sym: "pokefirered.sym",
    repo: "pokefirered",
    game: "Pokemon FireRed",
  },
  "729041b940afe031302d630fdbe57c0c145f3f7b6d9b8eca5e98678d0ca4d059": {
    sym: "pokefirered_rev1.sym",
    repo: "pokefirered",
    game: "Pokemon FireRed v1.1",
  },
  "78d310d557ceebc593bd393acc52d1b19a8f023fec40bc200e6063880d8531fc": {
    sym: "pokeleafgreen.sym",
    repo: "pokefirered",
    game: "Pokemon LeafGreen",
  },
  "2f978f635b9593f6ca26ec42481c53a6b39f6cddd894ad5c062c1419fac58825": {
    sym: "pokeleafgreen_rev1.sym",
    repo: "pokefirered",
    game: "Pokemon LeafGreen v1.1",
  },
};

interface SymResult {
  filename: string;
  symFilename: string;
  gameName: string;
}

async function loadSymFile(romBuffer: ArrayBuffer): Promise<SymResult> {
  if (!elements.status) {
    throw new Error("Missing status element");
  }
  elements.status.textContent = "Identifying ROM...";

  const romHash = await sha256(romBuffer);
  const romInfo = knownRoms[romHash];

  if (!romInfo) {
    throw new Error(`Unidentified ROM. SHA256: ${romHash}. This ROM is not supported.`);
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
  if (!elements.status || !elements.progressBar || !elements.versionText) {
    return;
  }
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
  await pyodide.loadPackage("micropip");
  const micropip = pyodide.pyimport("micropip");

  // await micropip.install("hypercutter");
  await micropip.install("http://localhost:8000/dist/hypercutter-0.2.1-py3-none-any.whl");

  const freeze = JSON.parse(micropip.freeze());
  elements.versionText.textContent = `v${freeze.packages.hypercutter.version}`;

  isReady = true;
  elements.status.textContent = "Ready";
  elements.status.className = "";
  if (elements.btn && elements.romInput) {
    elements.btn.disabled = !!elements.romInput!.files![0];
  }
}

function runExtraction(): void {
  if (!elements.form || !elements.btn || !elements.romInput) return;

  elements.form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!isReady) return;

    const format = (document.querySelector('input[name="format"]:checked') as HTMLInputElement)
      .value;
    if (elements.status && elements.output && elements.statusArticle && elements.btn) {
      elements.status.textContent = "Extracting...";
      elements.status.className = "loading";
      elements.output.style.display = "none";
      elements.statusArticle.style.display = "block";
      elements.btn.disabled = true;
    }

    try {
      if (!elements.romInput!.files![0]) {
        throw new Error("No file selected");
      }
      const file = elements.romInput!.files![0];
      const romBuffer = await file.arrayBuffer();
      const extractorCode = pycode;

      if (!pyodide) {
        throw new Error("Pyodide not initialized");
      }

      const importlib = pyodide.pyimport("importlib");
      const pathlib = pyodide.pyimport("pathlib");

      pathlib.Path("extractor.py").write_text(extractorCode);
      importlib.invalidate_caches();

      const { filename: symPath } = await loadSymFile(romBuffer);
      const romFilename = "/tmp/rom.gba";
      pyodide.FS.writeFile(romFilename, new Uint8Array(romBuffer));

      const extractor = pyodide.pyimport("extractor");
      const mt = extractor.extract_metatiles(symPath, romFilename);
      pathlib.Path("/tmp/metatiles.json").write_text(JSON.stringify(mt));

      if (format === "json" || format === "both") {
        if (pyodide) {
          downloadFile(
            new Uint8Array(pyodide.FS.readFile("/tmp/metatiles.json")),
            "metatiles.json",
            "application/json",
          );
        }
      }

      if (format === "png" || format === "both") {
        if (elements.status && elements.progressBar) {
          elements.status.textContent = "Rendering images...";
          elements.progressBar.style.display = "block";
          elements.progressBar.value = 0;
        }

        const jsonModule = pyodide!.pyimport("json");
        const metatilesData = jsonModule.loads(pathlib.Path("/tmp/metatiles.json").read_text());
        extractor.render_images(metatilesData, romFilename);

        downloadFile(
          new Uint8Array(pyodide!.FS.readFile("/tmp/tilesets.zip")),
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
        if (elements.statusArticle) {
          elements.statusArticle.style.display = "none";
        }
      }, 2500);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      if (elements.status && elements.progressBar) {
        elements.status.textContent = `Error: ${msg}`;
        elements.status.className = "error";
        elements.progressBar.style.display = "none";
      }
    } finally {
      if (elements.btn) {
        elements.btn.disabled = false;
      }
    }
  });

  elements.romInput.addEventListener("change", () => {
    if (elements.btn && elements.romInput && elements.romInput.files) {
      elements.btn.disabled = !isReady || !elements.romInput.files[0];
    }
  });

  if (elements.aboutDialogTrigger && elements.aboutDialog) {
    elements.aboutDialogTrigger.addEventListener("click", () => {
      elements.aboutDialog!.showModal();
    });

    const closeBtn = elements.aboutDialog.querySelector(
      'button[rel="prev"]',
    ) as HTMLButtonElement | null;
    if (closeBtn) {
      closeBtn.addEventListener("click", () => {
        elements.aboutDialog!.close();
      });
    }
  }
}

function init(): void {
  runExtraction();
  void initPyodide();
}

init();
