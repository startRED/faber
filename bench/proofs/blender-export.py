"""Headless export and round-trip proof using disposable Blender geometry."""

from argparse import ArgumentParser
import json
from pathlib import Path
import sys

import bpy
from mathutils import Vector


def box(name, size, position, material_name, color):
    bpy.ops.mesh.primitive_cube_add(size=1, location=position)
    shape = bpy.context.object
    shape.name = name
    shape.dimensions = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    material = bpy.data.materials.new(material_name)
    material.diffuse_color = (*color, 1)
    if material.node_tree is not None:
        shader = material.node_tree.nodes.get("Principled BSDF")
        if shader is not None:
            shader.inputs["Base Color"].default_value = (*color, 1)
    shape.data.materials.append(material)
    return shape


def emit(folder):
    folder.mkdir(parents=True, exist_ok=True)
    if any(folder.iterdir()):
        raise ValueError(f"Use an empty fixture directory: {folder}")
    bpy.context.scene.unit_settings.system = "NONE"
    base = box("Body", (4, 2, 1), (0, 0, 0.5), "Slate", (0.2, 0.3, 0.4))
    marker = box("FrontMarker", (0.25, 0.5, 0.75), (1.25, -0.7, 1.1), "Lime", (0.1, 0.8, 0.2))
    box("Excluded", (1, 1, 1), (20, 0, 0), "Pink", (0.8, 0.2, 0.7))
    bpy.ops.object.select_all(action="DESELECT")
    base.select_set(True)
    marker.select_set(True)
    bpy.ops.wm.save_as_mainfile(filepath=str(folder / "fixture.blend"))
    fbx = dict(use_selection=True, axis_forward="Z", axis_up="Y", apply_scale_options="FBX_SCALE_UNITS", bake_anim=False)
    bpy.ops.export_scene.fbx(filepath=str(folder / "asset.fbx"), **fbx)
    bpy.ops.export_scene.fbx(filepath=str(folder / "wrong-scale.fbx"), global_scale=0.01, **fbx)
    bpy.ops.export_scene.gltf(filepath=str(folder / "asset.glb"), export_format="GLB", use_selection=True)


def verify(path):
    if path.suffix == ".fbx":
        bpy.ops.import_scene.fbx(filepath=str(path))
    else:
        bpy.ops.import_scene.gltf(filepath=str(path))
    meshes = {o.name: o for o in bpy.context.scene.objects if o.type == "MESH"}
    assert set(meshes) == {"Body", "FrontMarker"}, f"Unexpected meshes: {set(meshes)}"
    samples = {}
    for name, material, size, center in [
        ("Body", "Slate", (4, 2, 1), (0, 0, 0.5)),
        ("FrontMarker", "Lime", (0.25, 0.5, 0.75), (1.25, -0.7, 1.1)),
    ]:
        mesh = meshes[name]
        corners = [mesh.matrix_world @ Vector(corner) for corner in mesh.bound_box]
        low = [min(p[i] for p in corners) for i in range(3)]
        high = [max(p[i] for p in corners) for i in range(3)]
        actual_size = [round(b - a, 5) for a, b in zip(low, high)]
        actual_center = [round((b + a) / 2, 5) for a, b in zip(low, high)]
        materials = [slot.material.name for slot in mesh.material_slots]
        assert actual_size == list(size), f"{name} size {actual_size} instead of {size}"
        assert actual_center == list(center), f"{name} center {actual_center} instead of {center}"
        assert materials == [material], f"{name} uses {materials}"
        samples[name] = {"size": actual_size, "center": actual_center, "materials": materials}
    print("BLENDER_PROOF " + json.dumps({"version": bpy.app.version_string, "file": path.name, "objects": samples}))


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("emit", "verify"))
    parser.add_argument("path", type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:])
    path = args.path.resolve()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if args.mode == "emit":
        emit(path)
    else:
        if path.suffix not in {".fbx", ".glb"} or not path.is_file():
            parser.error("verify needs an existing FBX or GLB file")
        verify(path)


if __name__ == "__main__":
    main()
