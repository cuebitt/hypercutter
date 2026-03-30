# hypercutter

`hypercutter` is a binary extraction tool for GBA Pokemon ROMs. Using `hypercutter`, you can distribute your Pokemon fangame without including copyrighted assets!

## Features

- Extract tiles, palettes, and metatiles from GBA Pokemon ROMs
- Supports: Pokemon Emerald, FireRed, LeafGreen, Ruby, Sapphire
- Decompress LZ77-compressed tileset data
- Render metatiles as PNG images
- CLI and Python API
- Pyodide-based web version

### Roadmap

- [ ] Support GBA ROMs
  - [x] Fire Red/Leaf Green
  - [ ] Ruby/Sapphire
  - [x] Emerald
- [ ] Dump sprites
  - [ ] Characters
  - [ ] Pokemon
  - [ ] Other (fonts, etc)
- [ ] Dump music (maybe)

## Requirements

- Python 3.10+
- GBA Pokemon ROM (Emerald, FireRed, LeafGreen, Ruby, or Sapphire - not included)
- ROM-specific [memory map](https://github.com/pret/pokeemerald/tree/symbols) (optional, auto-downloaded)

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
hypercutter -g emerald pokeemerald.gba
```

Options:

- `-g, --game` - Game to use (emerald, firered, leafgreen, ruby, sapphire)
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
- [pret/pokeemerald](https://github.com/pret/pokeemerald/tree/symbols), [pret/pokefirered](https://github.com/pret/pokefirered/tree/symbols), [pret/pokeruby](https://github.com/pret/pokeruby/tree/symbols)
  - Only the memory maps are used. This project contains no content from any Pokemon ROM dump.
