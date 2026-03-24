let pyodide = null;
let isReady = false;

const output = document.getElementById('output');
const statusView = document.getElementById('status');

const jsonForm = document.getElementById('json-form');
const pngForm = document.getElementById('png-form');
const jsonBtn = document.getElementById('json-btn');
const pngBtn = document.getElementById('png-btn');

const symFileJson = document.getElementById('sym-file-json');
const romFileJson = document.getElementById('rom-file-json');
const jsonFilePng = document.getElementById('json-file-png');
const romFilePng = document.getElementById('rom-file-png');

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
    // await micropip.install('http://localhost:8000/dist/hypercutter-0.2.0-py3-none-any.whl');
    await micropip.install('Pillow');

    isReady = true;
    statusView.textContent = 'Ready';
    statusView.className = '';
    updateButtons();
}

function updateButtons() {
    jsonBtn.disabled = !isReady || !symFileJson.files[0] || !romFileJson.files[0];
    pngBtn.disabled = !isReady || !jsonFilePng.files[0] || !romFilePng.files[0];
}

symFileJson.addEventListener('change', updateButtons);
romFileJson.addEventListener('change', updateButtons);
jsonFilePng.addEventListener('change', updateButtons);
romFilePng.addEventListener('change', updateButtons);

async function writeFiles(symFile, romFile) {
    const offsetsBuffer = await symFile.arrayBuffer();
    const romBuffer = await romFile.arrayBuffer();
    try {
        pyodide.FS.unlink('/tmp/emerald_offsets.yaml');
    } catch (e) { }
    try {
        pyodide.FS.unlink('/tmp/pokeemerald.gba');
    } catch (e) { }
    try {
        pyodide.FS.unlink('/tmp/metatiles.json');
    } catch (e) { }
    pyodide.FS.writeFile('/tmp/emerald_offsets.yaml', new Uint8Array(offsetsBuffer));
    pyodide.FS.writeFile('/tmp/pokeemerald.gba', new Uint8Array(romBuffer));
}

jsonForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isReady) return;

    statusView.textContent = 'Extracting...';
    statusView.className = 'loading';
    output.style.display = 'none';
    jsonBtn.disabled = true;

    try {
        await writeFiles(symFileJson.files[0], romFileJson.files[0]);

        await pyodide.runPythonAsync(`
from hypercutter import extract
import json
import os
import sys

os.makedirs('/tmp/output', exist_ok=True)

def strip_raw(obj):
    if isinstance(obj, dict):
        return {k: strip_raw(v) for k, v in obj.items() if not k.endswith('_raw')}
    elif isinstance(obj, list):
        return [strip_raw(i) for i in obj]
    return obj

try:
    metatiles = extract('/tmp/emerald_offsets.yaml', '/tmp/pokeemerald.gba')
    print(f'Extracted {len(metatiles)} metatiles')

    cleaned = strip_raw(metatiles)
    with open('/tmp/metatiles.json', 'w') as f:
        json.dump(cleaned, f, indent=2)

    print('Saved to /tmp/metatiles.json')
except Exception as e:
    print(f'Error: {e}', file=sys.stderr)
    raise
`);
        const jsonData = pyodide.FS.readFile('/tmp/metatiles.json');
        console.log('JSON data length:', jsonData.length);
        const jsonBlob = new Blob([jsonData], { type: 'application/json' });
        const jsonUrl = URL.createObjectURL(jsonBlob);
        const a = document.createElement('a');
        a.href = jsonUrl;
        a.download = 'metatiles.json';
        a.click();
        URL.revokeObjectURL(jsonUrl);

        statusView.textContent = 'Done';
        statusView.className = '';
        output.textContent = 'Extraction complete';
        output.style.display = 'block';
    } catch (error) {
        statusView.textContent = `Error: ${error.message}`;
        statusView.className = 'error';
        output.style.display = 'none';
    } finally {
        jsonBtn.disabled = false;
        updateButtons();
    }
});

pngForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isReady) return;

    statusView.textContent = 'Extracting...';
    statusView.className = 'loading';
    output.style.display = 'none';
    pngBtn.disabled = true;

    try {
        const jsonFile = jsonFilePng.files[0];
        const romFile = romFilePng.files[0];

        const jsonBuffer = await jsonFile.arrayBuffer();
        const romBuffer = await romFile.arrayBuffer();

        pyodide.FS.writeFile('/tmp/metatiles.json', new Uint8Array(jsonBuffer));
        pyodide.FS.writeFile('/tmp/pokeemerald.gba', new Uint8Array(romBuffer));

        await pyodide.runPythonAsync(`
import json
import zipfile
from hypercutter.renderer import TilesetRenderer
import os
import sys

os.makedirs('/tmp/output', exist_ok=True)

with open('/tmp/pokeemerald.gba', 'rb') as f:
    rom_data = f.read()

with open('/tmp/metatiles.json', 'r') as f:
    metatiles = json.load(f)

print(f'Loaded {len(metatiles)} metatiles')

for name, data in metatiles.items():
    try:
        renderer = TilesetRenderer(data, rom_data)
        img = renderer.render()
        img.save(f'/tmp/output/{name}.png')
        print(f'Saved {name}.png')
    except Exception as e:
        print(f'Error rendering {name}: {e}')

with zipfile.ZipFile('/tmp/tilesets.zip', 'w') as zf:
    for name in os.listdir('/tmp/output'):
        if name.endswith('.png'):
            zf.write(f'/tmp/output/{name}', name)
print('Created tilesets.zip')
`);
        console.log('Python execution complete');
        const zipData = pyodide.FS.readFile('/tmp/tilesets.zip');
        const zipBlob = new Blob([zipData], { type: 'application/zip' });
        const zipUrl = URL.createObjectURL(zipBlob);
        const a = document.createElement('a');
        a.href = zipUrl;
        a.download = 'tilesets.zip';
        a.click();
        URL.revokeObjectURL(zipUrl);

        statusView.textContent = 'Done';
        statusView.className = '';
        output.textContent = 'Extraction complete';
        output.style.display = 'block';
    } catch (error) {
        statusView.textContent = `Error: ${error.message}`;
        statusView.className = 'error';
        output.style.display = 'none';
    } finally {
        pngBtn.disabled = false;
        updateButtons();
    }
});

initPyodide();
