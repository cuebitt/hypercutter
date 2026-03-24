# hypercutter

Extract and render tilesets from Pokemon Emerald GBA ROM dumps.

## Features

- Extract tiles, palettes, and metatiles from Pokemon Emerald ROMs
- Decompress LZ77-compressed tileset data
- Render metatiles as PNG images
- CLI and Python API

## Requirements

- Python 3.10+
- Pokemon Emerald ROM and symbol file

## Installation

```bash
pip install hypercutter
```

Or with `pipx`/`uvx` for isolated execution:

```bash
pipx run hypercutter
uvx hypercutter
```

## Usage

```bash
hypercutter data/pokeemerald.sym data/pokeemerald.gba
```

Options:
- `-o, --output` - Output JSON path
- `-e, --export` - Directory for PNG exports
- `-v, --verbose` - Verbose output

## Documentation

See [docs.readthedocs.io](https://hypercutter.readthedocs.io)

## License

MIT

## Attribution

The following open-source libraries are used:

- [magical/nlzss](https://github.com/magical/nlzss)