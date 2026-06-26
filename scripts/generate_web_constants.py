"""Generate web/src/constants.ts from Python ROM_REVISIONS in classes.py."""

import sys
from pathlib import Path

# Add src to path so we can import the module
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from hypercutter.classes import ROM_REVISIONS, SUPPORTED_GAMES

# Map game codes to display names and repo info
GAME_INFO = {
    code: (profile.name, profile.sym_repo) for code, profile in SUPPORTED_GAMES.items()
}

# Map sym filenames to game display names with revision info
SYM_TO_DISPLAY: dict[str, tuple[str, str, str]] = {}
for game_code, revisions in ROM_REVISIONS.items():
    game_name, repo = GAME_INFO[game_code]
    for sha, sym_filename in revisions.items():
        # Determine display name
        if "rev1" in sym_filename:
            display = f"{game_name} v1.1"
        elif "rev2" in sym_filename:
            display = f"{game_name} v1.2"
        else:
            display = game_name
        SYM_TO_DISPLAY[sha] = (sym_filename, repo, display)

# Generate TypeScript
lines = [
    "export interface RomInfo {",
    "  sym: string;",
    "  repo: string;",
    "  game: string;",
    "}",
    "",
    "export const KNOWN_ROMS: Record<string, RomInfo> = {",
]

for sha, (sym, repo, game) in sorted(SYM_TO_DISPLAY.items(), key=lambda x: x[1][2]):
    # Quote keys that start with digits
    key = f'"{sha}"' if sha[0].isdigit() else sha
    lines.append(f"  {key}: {{")
    lines.append(f'    sym: "{sym}",')
    lines.append(f'    repo: "{repo}",')
    lines.append(f'    game: "{game}",')
    lines.append("  },")

lines.append("};")
lines.append("")
lines.append("export interface SymResult {")
lines.append("  filename: string;")
lines.append("  symFilename: string;")
lines.append("  gameName: string;")
lines.append("}")
lines.append("")

output = "\n".join(lines)
target = Path(__file__).resolve().parent.parent / "web" / "src" / "constants.ts"
target.write_text(output)
print(f"Generated {target}")
