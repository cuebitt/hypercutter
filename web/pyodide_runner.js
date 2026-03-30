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

// Helper: Compute SHA256 using Web Crypto API
async function sha256(buffer) {
    const hashBuffer = await crypto.subtle.digest('SHA-256', buffer);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}

// Helper: Cache sym file
async function loadSymFile(romBuffer) {
    elements.status.textContent = 'Identifying ROM...';

    // Compute SHA256 hash using Web Crypto API
    const romHash = await sha256(romBuffer);

    // Map of SHA256 hashes to {symFilename, repo, gameName}
    const knownRoms = {
        '53d591215de2cab847d14fbcf8c516f0128cfa8556f1236065e0535aa5936d4e': { sym: 'pokeruby.sym', repo: 'pokeruby', game: 'Pokemon Ruby' },
        '0d80909998a901c7edef5942068585bc855a85aec7e083aa6aeff84a5b2f8ec0': { sym: 'pokeruby_rev1.sym', repo: 'pokeruby', game: 'Pokemon Ruby v1.1' },
        '0fdd36e92b75bed65d09df4635ab0b707b288c2bf1dc4c6e7a4a4f0eebe9d64c': { sym: 'pokeruby_rev2.sym', repo: 'pokeruby', game: 'Pokemon Ruby v1.2' },
        'c36c1b899503e8823ee7eb607eea583adcef7ea92ff804838b193c227f2c6657': { sym: 'pokesapphire.sym', repo: 'pokeruby', game: 'Pokemon Sapphire' },
        '2f680a43e5c57aede4cb3b2cb04f7e15079efc122c88edaacfd6026db6e920ac': { sym: 'pokesapphire_rev1.sym', repo: 'pokeruby', game: 'Pokemon Sapphire v1.1' },
        '02ca41513580a8b780989dee428df747b52a0b1a55bec617886b4059eb1152fb': { sym: 'pokesapphire_rev2.sym', repo: 'pokeruby', game: 'Pokemon Sapphire v1.2' },
        'a9dec84dfe7f62ab2220bafaef7479da0929d066ece16a6885f6226db19085af': { sym: 'pokeemerald.sym', repo: 'pokeemerald', game: 'Pokemon Emerald' },
        '3d0c79f1627022e18765766f6cb5ea067f6b5bf7dca115552189ad65a5c3a8ac': { sym: 'pokefirered.sym', repo: 'pokefirered', game: 'Pokemon FireRed' },
        '729041b940afe031302d630fdbe57c0c145f3f7b6d9b8eca5e98678d0ca4d059': { sym: 'pokefirered_rev1.sym', repo: 'pokefirered', game: 'Pokemon FireRed v1.1' },
        '78d310d557ceebc593bd393acc52d1b19a8f023fec40bc200e6063880d8531fc': { sym: 'pokeleafgreen.sym', repo: 'pokefirered', game: 'Pokemon LeafGreen' },
        '2f978f635b9593f6ca26ec42481c53a6b39f6cddd894ad5c062c1419fac58825': { sym: 'pokeleafgreen_rev1.sym', repo: 'pokefirered', game: 'Pokemon LeafGreen v1.1' },
    };

    const romInfo = knownRoms[romHash];
    if (!romInfo) {
        throw new Error(`Unidentified ROM. SHA256: ${romHash}. This ROM is not supported.`);
    }

    elements.status.textContent = `Detected: ${romInfo.game}`;

    const cacheKey = `hypercutter_sym_${romInfo.sym}`;
    const filename = `/tmp/${romInfo.sym}`;
    elements.status.textContent = 'Loading symbol file...';

    const cached = localStorage.getItem(cacheKey);
    if (cached) {
        pyodide.FS.writeFile(filename, cached);
        return { filename, symFilename: romInfo.sym, gameName: romInfo.game };
    }

    const url = `https://cdn.jsdelivr.net/gh/pret/${romInfo.repo}@symbols/${romInfo.sym}`;

    try {
        const response = await fetch(url);
        if (!response.ok) throw new Error('Failed to download');
        const text = await response.text();
        localStorage.setItem(cacheKey, text);
        pyodide.FS.writeFile(filename, text);
        return { filename, symFilename: romInfo.sym, gameName: romInfo.game };
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

    // await micropip.install('hypercutter');
    await micropip.install("http://localhost:8000/dist/hypercutter-0.2.1-py3-none-any.whl")

    // Set version text in header
    const freeze = JSON.parse(micropip.freeze());
    elements.versionText.textContent = `v${freeze["packages"]["hypercutter"]["version"]}`;

    isReady = true;
    elements.status.textContent = 'Ready';
    elements.status.className = '';
    elements.btn.disabled = !elements.romInput.files[0];
}

// Clean up temp files
async function cleanup(symFilename) {
    const files = [`/tmp/${symFilename}`, '/tmp/metatiles.json', '/tmp/tilesets.zip'];
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

        const importlib = await pyodide.pyimport("importlib");
        const pathlib = await pyodide.pyimport("pathlib");

        pathlib.Path("extractor.py").write_text(extractorCode);
        importlib.invalidate_caches();

        const { filename: symPath, symFilename, gameName } = await loadSymFile(romBuffer);
        const romFilename = `/tmp/rom.gba`;
        pyodide.FS.writeFile(romFilename, new Uint8Array(romBuffer));

        // Extract metatiles
        const extractor = await pyodide.pyimport('extractor');
        extractor.extract_metatiles(symPath, romFilename);

        // Download JSON
        if (format === 'json' || format === 'both') {
            downloadFile(pyodide.FS.readFile('/tmp/metatiles.json'), 'metatiles.json', 'application/json');
        }

        // Render PNGs
        if (format === 'png' || format === 'both') {
            elements.status.textContent = 'Rendering images...';
            elements.progressBar.style.display = 'block';
            elements.progressBar.value = 0;

            const jsonModule = await pyodide.pyimport('json');
            const metatilesData = jsonModule.loads(pathlib.Path('/tmp/metatiles.json').read_text());
            extractor.render_images(metatilesData, romFilename);

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
