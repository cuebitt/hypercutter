// DOM elements
const elements = {
    form: document.getElementById('extract-form'),
    btn: document.getElementById('extract-btn'),
    romInput: document.getElementById('rom-file'),
    status: document.getElementById('status'),
    statusArticle: document.getElementById('status-article'),
    output: document.getElementById('output'),
    progressBar: document.getElementById('progress-bar'),
    versionText: document.getElementById('version-text'),
    aboutDialog: document.getElementById('about-dlg'),
    aboutDialogTrigger: document.getElementById('about-dlg-trigger'),
};

let pyodide = null;
let isReady = false;

// Helper: Download file from Pyodide filesystem
function downloadFile(data, filename, mimeType) {
    const blob = new Blob([data], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
}

// Helper: Run Python code
async function runPython(code) {
    await pyodide.runPythonAsync(code);
}

// Helper: Cache sym file
async function loadSymFile() {
    const url = 'https://cdn.jsdelivr.net/gh/pret/pokeemerald@symbols/pokeemerald.sym';
    elements.status.textContent = 'Loading symbol file...';

    const cached = localStorage.getItem('hypercutter_sym');
    if (cached) {
        pyodide.FS.writeFile('/tmp/pokeemerald.sym', cached);
        return;
    }

    try {
        const response = await fetch(url);
        if (!response.ok) throw new Error('Failed to download');
        const text = await response.text();
        localStorage.setItem('hypercutter_sym', text);
        pyodide.FS.writeFile('/tmp/pokeemerald.sym', text);
    } catch (e) {
        throw new Error(`Failed to load sym file: ${e.message}`);
    }
}

// Initialize Pyodide
async function initPyodide() {
    elements.status.textContent = 'Loading Pyodide...';
    elements.status.className = 'loading';

    pyodide = await loadPyodide({ indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.29.3/full/' });

    // Expose progress bar updater to Python
    globalThis.updateProgressBar = (value) => { elements.progressBar.value = value; };

    elements.status.textContent = 'Installing package...';
    await pyodide.loadPackage("micropip");
    const micropip = pyodide.pyimport("micropip");
    await micropip.install('hypercutter');

    globalThis.setPackageVersionText = (value) => {
        elements.versionText.textContent = `v${value}`
    }

    await runPython(`
from js import setPackageVersionText
import hypercutter
import micropip
import json

j = json.loads(micropip.freeze())
hc_ver = j["packages"]["hypercutter"]["version"]

setPackageVersionText(hc_ver)
`);

    isReady = true;
    elements.status.textContent = 'Ready';
    elements.status.className = '';
    elements.btn.disabled = !elements.romInput.files[0];
}

// Clean up temp files
async function cleanup() {
    const files = ['/tmp/pokeemerald.sym', '/tmp/pokeemerald.gba', '/tmp/metatiles.json', '/tmp/tilesets.zip'];
    for (const path of files) {
        try { pyodide.FS.unlink(path); } catch (e) { /* ignore */ }
    }
}

// Main extraction handler
elements.form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isReady) return;

    const format = document.querySelector('input[name="format"]:checked').value;
    elements.status.textContent = 'Extracting...';
    elements.status.className = 'loading';
    elements.output.style.display = 'none';
    elements.statusArticle.style.display = 'block';
    elements.btn.disabled = true;

    try {
        // Load ROM and Python module
        const romBuffer = await elements.romInput.files[0].arrayBuffer();
        const extractorCode = await fetch('extractor.py').then(r => r.text());

        globalThis.extractorCode = extractorCode;

        await runPython(`
import importlib
from pathlib import Path
from js import extractorCode

Path('extractor.py').write_text(extractorCode)
importlib.invalidate_caches()
`);

        await cleanup();
        await loadSymFile();
        pyodide.FS.writeFile('/tmp/pokeemerald.gba', new Uint8Array(romBuffer));

        // Extract metatiles
        await runPython(`
import json
from extractor import extract_metatiles

metatiles = extract_metatiles('/tmp/pokeemerald.sym', '/tmp/pokeemerald.gba')
with open('/tmp/metatiles.json', 'w') as f:
    json.dump(metatiles, f, indent=2)
`);

        // Download JSON
        if (format === 'json' || format === 'both') {
            downloadFile(pyodide.FS.readFile('/tmp/metatiles.json'), 'metatiles.json', 'application/json');
        }

        // Render PNGs
        if (format === 'png' || format === 'both') {
            elements.status.textContent = 'Rendering images...';
            elements.progressBar.style.display = 'block';
            elements.progressBar.value = 0;

            await runPython(`
import json
from extractor import render_images

with open('/tmp/metatiles.json', 'r') as f:
    render_images(json.load(f), '/tmp/pokeemerald.gba')
`);

            downloadFile(pyodide.FS.readFile('/tmp/tilesets.zip'), 'tilesets.zip', 'application/zip');
        }

        elements.status.textContent = 'Done';
        elements.status.className = '';
        elements.output.textContent = `Extraction complete (${format} format)`;
        elements.output.style.display = 'block';
        elements.progressBar.style.display = 'none';

        setTimeout(() => { elements.statusArticle.style.display = 'none'; }, 2500);
    } catch (error) {
        elements.status.textContent = `Error: ${error.message}`;
        elements.status.className = 'error';
        elements.progressBar.style.display = 'none';
    } finally {
        elements.btn.disabled = false;
    }
});

elements.romInput.addEventListener('change', () => {
    elements.btn.disabled = !isReady || !elements.romInput.files[0];
});

elements.aboutDialogTrigger.addEventListener('click', () => {
    elements.aboutDialog.showModal();
});

elements.aboutDialog.querySelector("button[rel=prev]").addEventListener("click", () => {
    elements.aboutDialog.close();
});

initPyodide();
