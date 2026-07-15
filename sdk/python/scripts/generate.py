"""Generate checked-in Pydantic v2 DTOs from the Operations OpenAPI contract."""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = PACKAGE_ROOT.parents[1]
SPEC_PATH = REPO_ROOT / "openapi" / "templiqx-operations-v1.yaml"
OUTPUT_PATH = PACKAGE_ROOT / "src" / "templiqx_adapter" / "_generated" / "operations_v1.py"


def _apply_codegen_fixups(generated: str) -> str:
    """Repair generator output that is valid at runtime but rejected by mypy."""
    untyped_json_default = (
        "    aliases: Annotated[JsonValue | None, Field(validate_default=True)] = {}\n"
    )
    typed_json_default = (
        "    aliases: Annotated[JsonValue | None, Field(validate_default=True)] = Field(\n"
        "        default_factory=lambda: JsonValue({})\n"
        "    )\n"
    )
    if generated.count(untyped_json_default) != 1:
        raise RuntimeError("Expected exactly one generated untyped JSON default")
    return generated.replace(untyped_json_default, typed_json_default)


def _metadata(spec: bytes) -> str:
    source = spec.decode("utf-8")
    match = re.search(r"^  version:\s*([^\s#]+)\s*$", source, re.MULTILINE)
    if match is None:
        raise RuntimeError(f"Could not read info.version from {SPEC_PATH}")

    pyproject = tomllib.loads((PACKAGE_ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    sdk_version = pyproject["project"]["version"]
    digest = f"sha256:{hashlib.sha256(spec).hexdigest()}"
    return (
        "\n# Codegen metadata used by the compatibility self-check.\n"
        f"GENERATED_OPENAPI_VERSION = {match.group(1)!r}\n"
        f"GENERATED_OPENAPI_DIGEST = {digest!r}\n"
        f"GENERATED_SDK_VERSION = {sdk_version!r}\n"
    )


def _generate(destination: Path) -> None:
    subprocess.run(
        [
            sys.executable,
            "-m",
            "datamodel_code_generator",
            "--input",
            str(SPEC_PATH),
            "--input-file-type",
            "openapi",
            "--output",
            str(destination),
            "--output-model-type",
            "pydantic_v2.BaseModel",
            "--target-python-version",
            "3.11",
            "--openapi-scopes",
            "schemas",
            "--disable-timestamp",
            "--use-annotated",
            "--use-standard-collections",
            "--use-union-operator",
            "--formatters",
            "black",
            "isort",
        ],
        cwd=PACKAGE_ROOT,
        check=True,
    )
    generated = _apply_codegen_fixups(destination.read_text(encoding="utf-8"))
    destination.write_text(generated + _metadata(SPEC_PATH.read_bytes()), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail when the checked-in generated module differs from fresh output",
    )
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="templiqx-python-sdk-") as temp_dir:
        generated_path = Path(temp_dir) / "operations_v1.py"
        _generate(generated_path)
        next_content = generated_path.read_text(encoding="utf-8")

    if args.check:
        current = OUTPUT_PATH.read_text(encoding="utf-8") if OUTPUT_PATH.exists() else ""
        if current != next_content:
            print(f"Generated Python DTOs are stale: {OUTPUT_PATH}", file=sys.stderr)
            return 1
        print(f"Generated Python DTOs are current: {OUTPUT_PATH}")
        return 0

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(next_content, encoding="utf-8")
    print(f"Generated Python DTOs: {OUTPUT_PATH}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
