Usage
=====

CLI
---

Extract metatiles from a Pokemon Emerald ROM::

    hypercutter data/pokeemerald.sym data/pokeemerald.gba

This outputs:

- ``out/metatiles.json`` - Metadata about extracted tilesets
- ``out/tilesets/`` - Rendered PNG images for each metatile

Options:

- ``-o, --output PATH`` - Output JSON path (default: ``out/metatiles.json``)
- ``-e, --export DIR`` - Directory to export PNGs (default: ``out/tilesets``)
- ``-v, --verbose`` - Enable verbose output

Python API
----------

.. code-block:: python

    from hypercutter import extract
    from hypercutter.renderer import TilesetRenderer

    # Extract all metatiles from ROM
    metatiles = extract("data/pokeemerald.sym", "data/pokeemerald.gba")

    # Render a specific tileset as PNG
    for name, data in metatiles.items():
        renderer = TilesetRenderer(data)
        img = renderer.render()
        img.save(f"out/{name}.png")

ROM Requirements
----------------

You need:

1. Pokemon Emerald GBA ROM (``pokeemerald.gba``)
2. Symbol file (``pokeemerald.sym``) from the `pokeemerald repository <https://github.com/pret/pokeemerald>`_
