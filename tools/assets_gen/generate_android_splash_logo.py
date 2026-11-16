#!/usr/bin/env python3
"""Generate the Android 12 splash icon as a density-independent vector."""

from __future__ import annotations

import xml.etree.ElementTree as ET
from xml.sax.saxutils import quoteattr
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "rust/assets/logo/logo.svg"
OUTPUT = (
    ROOT
    / "rust/gui/apps/android/android/app/src/main/res/drawable/splash_logo.xml"
)

# Keep the logo at the same visual size and safe-area padding as the reviewed
# launch screen while rendering directly at the final pixel dimensions.
CONTENT_SCALE = 0.662
VIEWBOX_CENTER = 627.0


def gradient(
    start_x: float,
    start_y: float,
    end_x: float,
    end_y: float,
    stops: list[tuple[float, str]],
) -> str:
    items = "\n".join(
        f'                    <item android:color="{color}" '
        f'android:offset="{offset:g}" />'
        for offset, color in stops
    )
    return f"""                    <gradient
                        android:type="linear"
                        android:startX="{start_x:g}"
                        android:startY="{start_y:g}"
                        android:endX="{end_x:g}"
                        android:endY="{end_y:g}">
{items}
                    </gradient>"""


def path_data() -> list[str]:
    tree = ET.parse(SOURCE)
    root = tree.getroot()
    svg_ns = "{http://www.w3.org/2000/svg}"
    paths = [path.get("d") for path in root.findall(f"{svg_ns}path")]
    if any(path is None for path in paths):
        raise RuntimeError("Every source logo path must contain path data")
    return [path for path in paths if path is not None]


def main() -> None:
    stem, arch, _, lower_blue = path_data()

    # Android 24+ vectors support gradient fills, so the splash screen can
    # rasterize the paths directly at the display's real pixel density.
    stem_gradient = gradient(
        185,
        1203,
        233.24,
        241,
        [
            (0, "#FF086FE8"),
            (0.48, "#FF0788F5"),
            (1, "#FF0797FF"),
        ],
    )
    arch_gradient = gradient(
        201,
        51,
        1134,
        51,
        [
            (0, "#FF35B8FF"),
            (0.30, "#FF368BFA"),
            (0.55, "#FF3E5FF0"),
            (0.68, "#FF7540F5"),
            (0.80, "#FFB52BFA"),
            (1, "#FFD52BFF"),
        ],
    )
    lower_blue_gradient = gradient(
        480.9,
        799,
        872.76,
        589.56,
        [
            (0, "#E0174FE0"),
            (0.34, "#9E2D70F0"),
            (0.68, "#4D65A0FF"),
            (1, "#0065A0FF"),
        ],
    )

    vector = f"""<?xml version="1.0" encoding="utf-8"?>
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:aapt="http://schemas.android.com/aapt"
    android:width="288dp"
    android:height="288dp"
    android:viewportWidth="1254"
    android:viewportHeight="1254">
    <group
        android:pivotX="{VIEWBOX_CENTER:g}"
        android:pivotY="{VIEWBOX_CENTER:g}"
        android:scaleX="{CONTENT_SCALE:g}"
        android:scaleY="{CONTENT_SCALE:g}">
        <path
            android:pathData={quoteattr(stem)}
            android:fillType="nonZero">
            <aapt:attr name="android:fillColor">
{stem_gradient}
            </aapt:attr>
        </path>
        <path
            android:pathData={quoteattr(arch)}
            android:fillType="nonZero">
            <aapt:attr name="android:fillColor">
{arch_gradient}
            </aapt:attr>
        </path>
        <path
            android:pathData={quoteattr(lower_blue)}
            android:fillType="nonZero">
            <aapt:attr name="android:fillColor">
{lower_blue_gradient}
            </aapt:attr>
        </path>
    </group>
</vector>
"""
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(vector, encoding="utf-8")
    print(f"Wrote {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
