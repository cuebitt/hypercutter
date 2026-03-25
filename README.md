# hypercutter

`hypercutter` is a binary extraction tool for GBA Pokemon ROMs. Using `hypercutter`, you can distribute your Pokemon fangame without including copyrighted assets!

## Features

- Extract tiles, palettes, and metatiles from Pokemon Emerald ROMs
- Decompress LZ77-compressed tileset data
- Render metatiles as PNG images
- CLI and Python API
- Pyodide-based web version

### Roadmap

- [ ] Support other GBA ROMs
	- [ ] Fire Red/Leaf Green
	- [ ] Ruby/Sapphire
- [ ] Dump sprites
	- [ ] Characters
	- [ ] Pokemon
	- [ ] Other (fonts, etc)
- [ ] Dump music (maybe)

## Requirements

- Python 3.10+
- Pokemon Emerald ROM (not included)
- Pokemon Emerald [memory map](https://github.com/pret/pokeemerald/tree/symbols) (optional)

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
hypercutter pokeemerald.gba
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
- [pret/pokeemerald](https://github.com/pret/pokeemerald/tree/symbols)
	- Only the memory map is used. This project contains no content from any Pokemon ROM dump.