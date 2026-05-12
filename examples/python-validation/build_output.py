from __future__ import annotations

from pathlib import Path
import subprocess


def main() -> None:
    app_root = Path(__file__).resolve().parent
    repo_root = app_root.parent.parent
    input_path = repo_root / "examples" / "input.wit"
    descriptor_path = repo_root / "descriptors.bin"
    destination = app_root / "output.py"

    for path in (input_path, descriptor_path):
        if not path.exists():
            raise SystemExit(f"missing generator input: {path}")

    _ = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--",
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


if __name__ == "__main__":
    main()
