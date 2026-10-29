"""Inventory the original project without changing it; verify copied assets byte for byte."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess

FORK = Path(__file__).resolve().parents[2]
DEFAULT_SOURCE = FORK.parent
REVIEWED = {
    "MagicProgression", "ManaSystem", "MagicData", "CastingManager", "CastingState",
    "Spell", "FullSpell", "SpellCatalog", "SpellCombat", "SpellEngine", "FireSpells",
    "ReworkSpells", "MagicSchool", "RitualManager", "ArcanaConfig", "SparkSpell",
    "WaterBoltSpell", "StoneBoltSpell", "WindGustSpell", "ActionPayload", "VisualPayload",
}

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def revision(root):
    return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()

def generate(source):
    source = source.resolve()
    files = []
    for path in sorted((source / "src").rglob("*")):
        if not path.is_file():
            continue
        entry = {"path": path.relative_to(source).as_posix(), "sha256": sha(path), "bytes": path.stat().st_size}
        if path.suffix == ".java":
            text = path.read_text(encoding="utf-8-sig")
            entry.update(lines=len(text.splitlines()),
                         imports=re.findall(r"^import (.*);", text, re.M),
                         review="partial_manual_review" if path.stem in REVIEWED else "inventory_only")
        files.append(entry)
    catalog_path = source / "src/main/resources/data/arcana_four_elements/spells.json"
    spells = json.loads(catalog_path.read_text(encoding="utf-8-sig"))
    animations = json.loads((source / "src/main/resources/data/arcana_four_elements/cast_animations.json").read_text(encoding="utf-8-sig"))
    copies = []
    for path in sorted((FORK / "arcana/assets/arcana_four_elements").rglob("*")):
        if path.is_file():
            original = source / "src/main/resources/assets/arcana_four_elements" / path.relative_to(FORK / "arcana/assets/arcana_four_elements")
            if not original.is_file() or sha(path) != sha(original):
                raise RuntimeError(f"asset copy differs: {path}")
            copies.append(path.relative_to(FORK).as_posix())
    for name in ("spells.json", "cast_animations.json"):
        assert sha(FORK / "arcana/assets/data" / name) == sha(source / "src/main/resources/data/arcana_four_elements" / name)
    return {
        "audit_scope": "Initial compatibility audit and complete file inventory; not a completed semantic review of all source",
        "arcana_revision": revision(source), "horizon_base_revision": "7ebf7d26cbe58d65ef873767c639f144dc403554",
        "counts": dict(Counter(Path(f["path"]).suffix for f in files)),
        "schools": dict(Counter(s["school"] for s in spells)),
        "animation_bindings": len(animations["bindings"]),
        "verified_asset_copies": len(copies),
        "files": files,
        "spell_migration": [{"id": s["id"], "school": s["school"], "mechanic": s["mechanic"],
                             "definition": "byte_identical", "logic": "solar_pilot" if s["id"] == "solar_spear" else "pending",
                             "game_verification": "pending"} for s in spells],
    }

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    p.add_argument("--check", action="store_true")
    args = p.parse_args()
    output = FORK / "arcana/docs/source-inventory.json"
    report = generate(args.source)
    if args.check:
        previous = json.loads(output.read_text(encoding="utf-8"))
        if previous["files"] != report["files"]:
            raise SystemExit("Original Arcana source or resources changed since inventory")
    else:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("counts", "schools", "animation_bindings", "verified_asset_copies")}, ensure_ascii=False))

if __name__ == "__main__":
    main()
