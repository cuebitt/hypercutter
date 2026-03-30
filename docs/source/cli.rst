CLI Reference
=============

.. program:: hypercutter

.. describe:: hypercutter <sym> <rom>

   Extract metatiles from a GBA Pokemon ROM.

   :param sym: Path to .sym file
   :param rom: Path to .gba ROM file

.. option:: -g GAME, --game GAME

   Game to use for automatic symbol file download. Choices: ``emerald``, ``firered``, ``leafgreen``, ``ruby``, ``sapphire``

.. option:: -o PATH, --output PATH

   Output JSON path. Default: ``out/metatiles.json``

.. option:: -e DIR, --export DIR

   Directory to export PNGs. Default: ``out/tilesets``

.. option:: -v, --verbose

   Enable verbose output.

Examples
--------

Extract from a Pokemon Emerald ROM::

    hypercutter -g emerald data/pokeemerald.gba

Extract from a Pokemon FireRed ROM::

    hypercutter -g firered data/pokefirered.gba

Extract only JSON metadata::

    hypercutter data/pokeemerald.sym data/pokeemerald.gba -o output.json

Extract only PNG images::

    hypercutter data/pokefirered.sym data/pokefirered.gba -e images

Extract both::

    hypercutter data/pokeruby.sym data/pokeruby.gba
