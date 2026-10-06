"""Build the release Windows extension and type stub into texloader/."""

import os
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NATIVE = ROOT / "native"
TARGET = "i686-pc-windows-gnullvm"


def main() -> None:
    environment = dict(os.environ)
    for name in (
        "PYO3_BUILD_EXTENSION_MODULE",
        "PYO3_CROSS",
        "PYO3_CROSS_PYTHON_VERSION",
        "PYO3_CROSS_LIB_DIR",
    ):
        environment.pop(name, None)

    subprocess.run(
        ["cargo", "build", "--release", "-p", "texloader", "--lib", "--target", TARGET],
        cwd=NATIVE,
        env=environment | {
            "PYO3_CROSS": "1",
            "PYO3_CROSS_PYTHON_VERSION": "3.14",
            "PYO3_BUILD_EXTENSION_MODULE": "1",
        },
        check=True,
    )
    subprocess.run(
        ["cargo", "run", "--release", "-p", "texloader", "--bin", "stub_gen"],
        cwd=NATIVE,
        env=environment,
        check=True,
    )
    shutil.copy2(
        NATIVE / "target" / TARGET / "release" / "_texloader.dll",
        ROOT / "texloader" / "_texloader.pyd",
    )


if __name__ == "__main__":
    main()
