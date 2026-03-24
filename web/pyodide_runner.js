let pyodide = null;
let isReady = false;

const offsetsFileInput = document.getElementById('sym-file');
const romFileInput = document.getElementById('rom-file');
const output = document.getElementById('output');
const statusView = document.getElementById('status');

const metatileInfoForm = document.getElementById('metatile-info-form');
const runButton = metatileInfoForm.querySelector("input[type=submit]");

async function initPyodide() {
    statusView.textContent = 'Loading Pyodide...';
    statusView.className = 'loading';

    pyodide = await loadPyodide({
        indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.29.3/full/'
    });

    statusView.textContent = 'Installing package...';

    await pyodide.loadPackage('micropip');
    const micropip = pyodide.pyimport('micropip');
    await micropip.install('hypercutter');

    isReady = true;
    statusView.textContent = 'Ready';
    statusView.className = '';
    updateRunButton();
}

function updateRunButton() {
    runButton.disabled = !isReady || !offsetsFileInput.files[0] || !romFileInput.files[0];
}

offsetsFileInput.addEventListener('change', updateRunButton);
romFileInput.addEventListener('change', updateRunButton);


metatileInfoForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isReady) return;

    statusView.textContent = 'Extracting...';
    statusView.className = 'loading';
    output.style.display = 'none';
    runButton.disabled = true;

    try {
        const offsetsFile = offsetsFileInput.files[0];
        const romFile = romFileInput.files[0];

        const offsetsBuffer = await offsetsFile.arrayBuffer();
        const romBuffer = await romFile.arrayBuffer();

        const offsetsData = new Uint8Array(offsetsBuffer);
        const romData = new Uint8Array(romBuffer);

        pyodide.FS.writeFile('/tmp/emerald_offsets.yaml', offsetsData);
        pyodide.FS.writeFile('/tmp/pokeemerald.gba', romData);

        await pyodide.runPythonAsync(`
from hypercutter import extract
import json
import os

os.makedirs('/tmp/output', exist_ok=True)

metatiles = extract('/tmp/emerald_offsets.yaml', '/tmp/pokeemerald.gba')
print(f'Extracted {len(metatiles)} metatiles')

with open('/tmp/metatiles.json', 'w') as f:
    json.dump(metatiles, f, indent=2)

print('Saved to /tmp/metatiles.json')
`);

        const jsonBlob = new Blob([pyodide.FS.readFile('/tmp/metatiles.json')], { type: 'application/json' });
        const jsonUrl = URL.createObjectURL(jsonBlob);
        const a = document.createElement('a');
        a.href = jsonUrl;
        a.download = 'metatiles.json';
        a.click();
        URL.revokeObjectURL(jsonUrl);

        statusView.textContent = 'Done';
        statusView.className = '';
        output.textContent = 'Metatiles extracted to metatiles.json';
        output.style.display = 'block';
    } catch (error) {
        statusView.textContent = `Error: ${error.message}`;
        statusView.className = 'error';
        output.style.display = 'none';
    } finally {
        runButton.disabled = false;
        updateRunButton();
    }
});

initPyodide();
