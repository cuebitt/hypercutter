let pyodide = null;
let isReady = false;

const offsetsFileInput = document.getElementById('sym-file');
const romFileInput = document.getElementById('rom-file');
const runButton = document.getElementById('run');
const output = document.getElementById('output');
const status = document.getElementById('status');

async function initPyodide() {
    status.textContent = 'Loading Pyodide...';
    status.className = 'loading';

    pyodide = await loadPyodide({
        indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.29.3/full/'
    });

    status.textContent = 'Installing package...';

    await pyodide.loadPackage('micropip');
    const micropip = pyodide.pyimport('micropip');
    await micropip.install('hypercutter');

    isReady = true;
    status.textContent = 'Ready';
    status.className = '';
    updateRunButton();
}

function updateRunButton() {
    runButton.disabled = !isReady || !offsetsFileInput.files[0] || !romFileInput.files[0];
}

offsetsFileInput.addEventListener('change', updateRunButton);
romFileInput.addEventListener('change', updateRunButton);

runButton.addEventListener('click', async () => {
    if (!isReady) return;

    status.textContent = 'Extracting...';
    status.className = 'loading';
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
import os

os.makedirs('/tmp/output', exist_ok=True)

metatiles = extract('/tmp/emerald_offsets.yaml', '/tmp/pokeemerald.gba')
print(f'Extracted {len(metatiles)} metatiles')
for name in metatiles:
    print(name)
`);

        status.textContent = 'Done';
        status.className = '';
        output.textContent = 'Tiles extracted to /tmp/output/';
        output.style.display = 'block';
    } catch (error) {
        status.textContent = `Error: ${error.message}`;
        status.className = 'error';
        output.style.display = 'none';
    } finally {
        runButton.disabled = false;
        updateRunButton();
    }
});

initPyodide();
