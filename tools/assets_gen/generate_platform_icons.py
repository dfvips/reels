from pathlib import Path
import json
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[2]
LOGO = ROOT / "rust/assets/logo"
ANDROID_RES = ROOT / "rust/gui/apps/android/android/app/src/main/res"
IOS_ASSETS = ROOT / "rust/gui/apps/ios/ios/Reels/Assets.xcassets"

def resize(source: Path, output: Path, dimension: int) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["sips", "-z", str(dimension), str(dimension), str(source), "--out", str(output)],
        check=True,
        stdout=subprocess.DEVNULL,
    )


def copy_icon(source: Path, output: Path, dimension: int) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    resize(source, output, dimension)
    from PIL import Image

    image = Image.open(output).convert("RGBA")
    red, green, blue, _ = image.split()
    opaque = Image.merge("RGBA", (red, green, blue, Image.new("L", image.size, 255)))
    opaque.convert("RGB").save(output)


def generate_android() -> None:
    dimensions = {
        "mipmap-mdpi": 48,
        "mipmap-hdpi": 72,
        "mipmap-xhdpi": 96,
        "mipmap-xxhdpi": 144,
        "mipmap-xxxhdpi": 192,
    }
    for source, name in (
        (LOGO / "logo.png", "ic_launcher.png"),
        (LOGO / "logo-dark.png", "ic_launcher_dark.png"),
    ):
        for folder, dimension in dimensions.items():
            resize(source, ANDROID_RES / folder / name, dimension)


def write_ios_iconset(name: str, sources: list[tuple[Path, str, list[dict]]]) -> None:
    iconset = IOS_ASSETS / f"{name}.appiconset"
    images = []
    for source, filename, appearances in sources:
        copy_icon(source, iconset / filename, 1024)
        image = {
            "filename": filename,
            "idiom": "universal",
            "platform": "ios",
            "size": "1024x1024",
        }
        if appearances:
            image["appearances"] = appearances
        images.append(image)
    contents = {
        "images": images,
        "info": {
            "author": "xcode",
            "version": 1,
        },
    }
    (iconset / "Contents.json").write_text(json.dumps(contents, indent=2) + "\n")


def generate_ios() -> None:
    variants = [
        ([], ""),
        ([{"appearance": "luminosity", "value": "dark"}], "dark-"),
        ([{"appearance": "luminosity", "value": "tinted"}], "tinted-"),
    ]
    icon_sources = {
        "AppIcon": {
            "": LOGO / "logo-org.png",
            "dark-": LOGO / "logo-org-dark.png",
            "tinted-": LOGO / "logo-tinted.png",
        },
        "AppIconLight": {
            "": LOGO / "logo-org.png",
            "dark-": LOGO / "logo-org-dark.png",
            "tinted-": LOGO / "logo-tinted.png",
        },
        "AppIconDark": {
            "": LOGO / "logo-org-dark.png",
            "dark-": LOGO / "logo-org-dark.png",
            "tinted-": LOGO / "logo-tinted.png",
        },
    }
    for name, sources_by_prefix in icon_sources.items():
        sources = []
        for appearances, prefix in variants:
            sources.append(
                (
                    sources_by_prefix[prefix],
                    f"{prefix}Icon-App-1024x1024@1x.png",
                    appearances,
                )
            )
        write_ios_iconset(name, sources)


def copy_root_aliases() -> None:
    shutil.copy2(LOGO / "logo-dark.ico", ROOT / "rust/assets/icon.ico")
    shutil.copy2(LOGO / "logo.icns", ROOT / "rust/assets/macos.icns")
    shutil.copy2(LOGO / "logo-dark.icns", ROOT / "rust/assets/macos-dark.icns")


if __name__ == "__main__":
    generate_android()
    generate_ios()
    copy_root_aliases()
