#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
import xml.etree.ElementTree as ET

SVG_NS = "http://www.w3.org/2000/svg"
INKSCAPE_NS = "http://www.inkscape.org/namespaces/inkscape"
SODIPODI_NS = "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
CC_NS = "http://creativecommons.org/ns#"
DC_NS = "http://purl.org/dc/elements/1.1/"
RDF_NS = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"

ET.register_namespace("", SVG_NS)
ET.register_namespace("inkscape", INKSCAPE_NS)
ET.register_namespace("sodipodi", SODIPODI_NS)
ET.register_namespace("cc", CC_NS)
ET.register_namespace("dc", DC_NS)
ET.register_namespace("rdf", RDF_NS)

SCRIPT_DIR = Path(__file__).resolve().parent
DOCNAME_ATTR = f"{{{SODIPODI_NS}}}docname"


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def normalize_path_data(path_data: str | None) -> str:
    return " ".join((path_data or "").split())


def parse_style(style: str | None) -> dict[str, str]:
    values: dict[str, str] = {}
    if not style:
        return values
    for item in style.split(";"):
        if not item or ":" not in item:
            continue
        key, value = item.split(":", 1)
        values[key.strip()] = value.strip()
    return values


def find_by_id(root: ET.Element, element_id: str) -> ET.Element | None:
    for element in root.iter():
        if element.get("id") == element_id:
            return element
    return None


def direct_child_paths(group: ET.Element) -> list[ET.Element]:
    return [
        child
        for child in list(group)
        if local_name(child.tag) == "path" and child.get("id")
    ]


def descendant_paths(group: ET.Element) -> list[ET.Element]:
    return [
        element
        for element in group.iter()
        if local_name(element.tag) == "path" and element.get("id")
    ]


def parse_clip_reference(value: str | None) -> str | None:
    if not value:
        return None
    match = re.fullmatch(r"url\(#([^)]+)\)", value.strip())
    if match:
        return match.group(1)
    return None


def first_paint_value(element: ET.Element, name: str) -> str | None:
    style_value = parse_style(element.get("style")).get(name)
    if style_value:
        return style_value.lower()
    raw_value = element.get(name)
    if raw_value:
        return raw_value.lower()
    return None


def find_self_clipped_logo(root: ET.Element) -> dict[str, object]:
    matches: list[dict[str, object]] = []

    for group in root.iter():
        if local_name(group.tag) != "g":
            continue

        clip_path_id = parse_clip_reference(group.get("clip-path"))
        if not clip_path_id:
            continue

        art_paths = direct_child_paths(group)
        if not art_paths:
            continue

        clip_path = find_by_id(root, clip_path_id)
        if clip_path is None or local_name(clip_path.tag) != "clipPath":
            continue

        clip_paths = descendant_paths(clip_path)
        if len(art_paths) != len(clip_paths):
            continue

        art_d = [normalize_path_data(path.get("d")) for path in art_paths]
        clip_d = [normalize_path_data(path.get("d")) for path in clip_paths]
        if art_d != clip_d:
            continue

        clip_group = next(
            (
                element
                for element in clip_path.iter()
                if local_name(element.tag) == "g" and element.get("id")
            ),
            None,
        )

        fill_color = first_paint_value(art_paths[0], "fill") or first_paint_value(
            group, "fill"
        )
        stroke_color = first_paint_value(
            art_paths[0], "stroke"
        ) or first_paint_value(group, "stroke")
        if not fill_color or not stroke_color:
            raise RuntimeError("Could not determine the original fill and stroke colors.")

        matches.append(
            {
                "art_group_id": group.get("id"),
                "art_path_ids": [path.get("id") for path in art_paths],
                "clip_path_id": clip_path_id,
                "clip_path_ids": [path.get("id") for path in clip_paths],
                "released_clip_group_id": clip_group.get("id") if clip_group is not None else None,
                "fill_color": fill_color,
                "stroke_color": stroke_color,
            }
        )

    if not matches:
        raise RuntimeError("Could not find the clipped logo group in the source SVG.")
    if len(matches) > 1:
        raise RuntimeError("Found multiple clipped logo groups; refusing to guess.")
    return matches[0]


def action_select(ids: list[str]) -> list[str]:
    actions = ["select-clear"]
    actions.extend(f"select-by-id:{element_id}" for element_id in ids)
    return actions


def run_inkscape(
    inkscape_bin: str, input_svg: Path, output_svg: Path, actions: list[str]
) -> None:
    command = [
        inkscape_bin,
        f"--actions={';'.join(actions)}",
        "--export-type=svg",
        "--export-overwrite",
        f"--export-filename={output_svg}",
        str(input_svg),
    ]
    result = subprocess.run(command, text=True, capture_output=True)
    if result.returncode != 0:
        raise RuntimeError(
            "Inkscape failed.\n"
            f"Command: {' '.join(command)}\n"
            f"stdout:\n{result.stdout}\n"
            f"stderr:\n{result.stderr}"
        )


def classify_outlined_paths(
    outlined_svg: Path, art_group_id: str, fill_color: str, stroke_color: str
) -> tuple[list[str], list[str]]:
    root = ET.parse(outlined_svg).getroot()
    art_group = find_by_id(root, art_group_id)
    if art_group is None:
        raise RuntimeError("The outlined SVG no longer contains the logo group.")

    group_fill = first_paint_value(art_group, "fill")
    fill_ids: list[str] = []
    outline_ids: list[str] = []
    for path in direct_child_paths(art_group):
        path_fill = first_paint_value(path, "fill") or group_fill
        path_id = path.get("id")
        if not path_fill or not path_id:
            continue
        if path_fill == fill_color:
            fill_ids.append(path_id)
        elif path_fill == stroke_color:
            outline_ids.append(path_id)

    if not fill_ids or not outline_ids:
        raise RuntimeError(
            "Could not classify the outlined paths into fills and stroke outlines."
        )
    return fill_ids, outline_ids


def remove_element(root: ET.Element, target: ET.Element) -> bool:
    for parent in root.iter():
        for child in list(parent):
            if child is target:
                parent.remove(child)
                return True
    return False


def cleanup_svg(
    input_svg: Path,
    output_svg: Path,
    art_group_id: str,
    clip_path_id: str,
    released_clip_group_id: str | None,
) -> None:
    tree = ET.parse(input_svg)
    root = tree.getroot()

    art_group = find_by_id(root, art_group_id)
    if art_group is None:
        raise RuntimeError("The baked SVG no longer contains the logo group.")
    art_group.attrib.pop("clip-path", None)

    clip_path = find_by_id(root, clip_path_id)
    if clip_path is not None:
        remove_element(root, clip_path)

    if released_clip_group_id:
        released_group = find_by_id(root, released_clip_group_id)
        if released_group is not None and len(list(released_group)) == 0:
            remove_element(root, released_group)

    for defs in [element for element in root.iter() if local_name(element.tag) == "defs"]:
        if len(list(defs)) == 0:
            remove_element(root, defs)

    root.set(DOCNAME_ATTR, output_svg.name)
    ET.indent(tree, space="  ")
    tree.write(output_svg, encoding="UTF-8", xml_declaration=True)


def bake_logo(input_svg: Path, output_svg: Path, inkscape_bin: str) -> None:
    if shutil.which(inkscape_bin) is None:
        raise RuntimeError(f"Could not find Inkscape binary: {inkscape_bin}")
    if not input_svg.exists():
        raise RuntimeError(f"Input SVG does not exist: {input_svg}")

    source_info = find_self_clipped_logo(ET.parse(input_svg).getroot())
    output_svg.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="bake_logo_", dir=output_svg.parent) as tempdir:
        tempdir_path = Path(tempdir)
        released_svg = tempdir_path / "released.svg"
        outlined_svg = tempdir_path / "outlined.svg"
        baked_svg = tempdir_path / "baked.svg"
        cleaned_svg = tempdir_path / "cleaned.svg"

        run_inkscape(
            inkscape_bin,
            input_svg,
            released_svg,
            action_select([source_info["art_group_id"]]) + ["object-release-clip"],
        )

        run_inkscape(
            inkscape_bin,
            released_svg,
            outlined_svg,
            action_select(source_info["art_path_ids"]) + ["object-stroke-to-path", "selection-ungroup"],
        )

        fill_ids, outline_ids = classify_outlined_paths(
            outlined_svg,
            source_info["art_group_id"],
            source_info["fill_color"],
            source_info["stroke_color"],
        )

        run_inkscape(
            inkscape_bin,
            outlined_svg,
            baked_svg,
            action_select(outline_ids)
            + ["path-union"]
            + action_select(source_info["clip_path_ids"])
            + ["path-union"]
            + action_select([outline_ids[0], source_info["clip_path_ids"][0]])
            + ["path-intersection"]
            + action_select(fill_ids)
            + ["path-union"],
        )

        cleanup_svg(
            baked_svg,
            cleaned_svg,
            source_info["art_group_id"],
            source_info["clip_path_id"],
            source_info["released_clip_group_id"],
        )

        cleaned_svg.replace(output_svg)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Bake the self-clipped logo into a plain paths-only SVG."
    )
    parser.add_argument(
        "input",
        nargs="?",
        type=Path,
        default=SCRIPT_DIR / "logo-with-clip.svg",
        help="Source SVG that still contains the clip path.",
    )
    parser.add_argument(
        "output",
        nargs="?",
        type=Path,
        default=SCRIPT_DIR / "logo.svg",
        help="Output SVG with only plain path geometry.",
    )
    parser.add_argument(
        "--inkscape-bin",
        default="inkscape",
        help="Inkscape executable to use.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        bake_logo(args.input.resolve(), args.output.resolve(), args.inkscape_bin)
    except Exception as exc:  # pragma: no cover - CLI error path
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print(args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
