CLI Reference
=============

.. program:: hypercutter

.. describe:: hypercutter <sym> <rom>

   Extract metatiles from a Pokemon Emerald ROM.

   :param sym: Path to .sym file
   :param rom: Path to .gba ROM file

.. option:: -o PATH, --output PATH

   Output JSON path. Default: ``out/metatiles.json``

.. option:: -e DIR, --export DIR

   Directory to export PNGs. Default: ``out/tilesets``

.. option:: -v, --verbose

   Enable verbose output.

Examples
--------

Extract only JSON metadata::

    hypercutter data/pokeemerald.sym data/pokeemerald.gba -o output.json

Extract only PNG images::

    hypercutter data/pokeemerald.sym data/pokeemerald.gba -e images

Extract both::

    hypercutter data/pokeemerald.sym data/pokeemerald.gba
