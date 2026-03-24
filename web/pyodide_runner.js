let pyodide = null;
let isReady = false;

const symFileInput = document.getElementById('sym-file');
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

    await pyodide.loadPackage("micropip");
    const micropip = pyodide.pyimport('micropip');
    await micropip.install("hypercutter");

    const { extract } = pyodide.pyimport('extract_data');
    pyodide.extract = extract;

    isReady = true;
    status.textContent = 'Ready';
    status.className = '';
    updateRunButton();
}

function updateRunButton() {
    runButton.disabled = !isReady || !symFileInput.files[0] || !romFileInput.files[0];
}

symFileInput.addEventListener('change', updateRunButton);
romFileInput.addEventListener('change', updateRunButton);

runButton.addEventListener('click', async () => {
    if (!isReady) return;

    status.textContent = 'Extracting...';
    status.className = 'loading';
    output.style.display = 'none';
    runButton.disabled = true;

    try {
        const symFile = symFileInput.files[0];
        const romFile = romFileInput.files[0];

        const symBuffer = await symFile.arrayBuffer();
        const romBuffer = await romFile.arrayBuffer();

        const symData = new Uint8Array(symBuffer);
        const romData = new Uint8Array(romBuffer);

        const result = pyodide.runPython(`
import json
json.dumps(pyodide.extract(sym_data.to_py(), rom_data.to_py()))
        `, { globals: pyodide.toPy({ sym_data: symData, rom_data: romData }) });

        output.textContent = JSON.stringify(JSON.parse(result), null, 2);
        output.style.display = 'block';
        status.textContent = 'Done';
        status.className = '';
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
