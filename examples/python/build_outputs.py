from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys


def main() -> None:
    app_root = Path(__file__).resolve().parent
    repo_root = app_root.parent.parent
    inputs_root = repo_root / "examples" / "inputs"
    descriptor_path = repo_root / "examples" / "descriptors" / "temporal_api.bin"

    example_ids = sys.argv[1:] or discover_example_ids(app_root, inputs_root)
    if not example_ids:
        raise SystemExit("no Python examples found")

    for example_id in example_ids:
        input_path = input_path_for_example(inputs_root, example_id)
        destination = app_root / example_id / "output.py"
        for path in (input_path, descriptor_path):
            if not path.exists():
                raise SystemExit(f"missing generator input: {path}")

        destination.parent.mkdir(parents=True, exist_ok=True)
        _ = subprocess.run(
            generator_command()
            + [
                "generate",
                "--lang",
                "python",
                "--input",
                str(input_path),
                "--descriptors",
                str(descriptor_path),
                "--output",
                str(destination),
                "--format",
            ],
            check=True,
            cwd=repo_root,
        )
        print(f"Built {destination} with nexus-api-gen")


def discover_example_ids(app_root: Path, inputs_root: Path) -> list[str]:
    example_ids = {
        input_path.stem
        for input_path in inputs_root.glob("*.wit")
        if (app_root / input_path.stem).is_dir()
    }
    example_ids.update(
        input_dir.name
        for input_dir in inputs_root.iterdir()
        if input_dir.is_dir()
        and (input_dir / "main.wit").is_file()
        and (app_root / input_dir.name).is_dir()
    )
    return sorted(example_ids)


def input_path_for_example(inputs_root: Path, example_id: str) -> Path:
    flat_path = inputs_root / f"{example_id}.wit"
    if flat_path.is_file():
        return flat_path

    nested_path = inputs_root / example_id / "main.wit"
    if nested_path.is_file():
        return nested_path

    return flat_path


def generator_command() -> list[str]:
    if configured_binary := os.environ.get("NEXUS_API_GEN_BIN"):
        return [configured_binary]

    return ["cargo", "run", "--quiet", "--"]


if __name__ == "__main__":
    main()
