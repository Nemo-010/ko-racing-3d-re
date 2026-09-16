"""Render a track the port built, so it can be looked at.

The port cannot start without a display, but it can *build*: `scene::build`
works on plain data, so `cargo run --bin dump_track` writes what it built to a
text file without ever making a window.  This module draws that file.

That ordering matters.  Reading the port's own output rather than rebuilding the
scene here means what gets rendered is what the port draws - the same
conversions, the same place it puts each tile, the same texture coordinates -
and the only thing added is the camera and a rasteriser.

```sh
cd rust/kora && cargo run --release --bin dump_track -- assets 1.map /tmp/1.track
cd ../../tools && python3 -m kora view /tmp/1.track /tmp/1.png
```
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Sequence, Tuple

from . import pack, png

Vec3 = Tuple[float, float, float]
Vec2 = Tuple[float, float]
Triangle = Tuple[List[Vec3], List[Vec2]]

#: How close a vertex may come before it is clipped, matching the port's own
#: near plane.
NEAR = 0.05


class Dump:
    """What `dump_track` wrote: geometry by texture, the camera, and the grid."""

    def __init__(self) -> None:
        #: texture resource -> triangles, already placed in the world
        self.triangles: Dict[str, List[Triangle]] = {}
        #: texture resource -> (width, height, rgba)
        self.textures: Dict[str, Tuple[int, int, bytes]] = {}
        self.eye: Optional[Vec3] = None
        self.target: Optional[Vec3] = None
        self.fovy = 62.0
        self.slots: List[Tuple[Vec3, float]] = []

    def count(self) -> int:
        return sum(len(faces) for faces in self.triangles.values())

    def load(self, path: str, pack_dir: str = "../x") -> "Dump":
        resources = pack.ResourcePack.load(pack_dir)
        raw = {item.name: item.data for item in resources.iter_resources()}

        current: Optional[str] = None
        is_car = False
        points: List[Vec3] = []
        uvs: List[Vec2] = []
        car_points: List[Vec3] = []
        car_uvs: List[Vec2] = []
        car_texture: Optional[str] = None

        with open(path) as fh:
            for line in fh:
                fields = line.split()
                if not fields:
                    continue
                if fields[0] == "TEX":
                    current, is_car = fields[1], False
                    self.triangles.setdefault(current, [])
                elif fields[0] == "CAR":
                    current, is_car = fields[1], True
                    car_texture = fields[1]
                elif fields[0] == "V":
                    point = (float(fields[1]), float(fields[2]), float(fields[3]))
                    uv = (float(fields[4]), float(fields[5]))
                    if is_car:
                        car_points.append(point)
                        car_uvs.append(uv)
                    else:
                        points.append(point)
                        uvs.append(uv)
                        if len(points) == 3:
                            self.triangles[current].append((points, uvs))
                            points, uvs = [], []
                elif fields[0] == "CAMERA":
                    self.eye = (float(fields[1]), float(fields[2]), float(fields[3]))
                    self.target = (float(fields[4]), float(fields[5]), float(fields[6]))
                    self.fovy = float(fields[7])
                elif fields[0] == "SLOT":
                    self.slots.append(
                        (
                            (float(fields[1]), float(fields[2]), float(fields[3])),
                            float(fields[4]),
                        )
                    )

        # The cars are dumped once, in their own space, and placed on the grid.
        if car_texture is not None and car_points:
            body = self.triangles.setdefault(car_texture, [])
            for start in range(0, len(car_points), 3):
                local = car_points[start : start + 3]
                local_uv = car_uvs[start : start + 3]
                for slot, yaw in self.slots:
                    body.append(([place(point, slot, yaw) for point in local], local_uv))

        for name in self.triangles:
            if name in raw and name not in self.textures:
                try:
                    self.textures[name] = png.read(raw[name])
                except Exception:
                    pass
        return self


def place(point: Vec3, origin: Vec3, yaw: float) -> Vec3:
    """A car vertex, turned about Y and moved onto its grid slot."""
    cos, sin = math.cos(yaw), math.sin(yaw)
    return (
        point[0] * cos + point[2] * sin + origin[0],
        point[1] + origin[1],
        -point[0] * sin + point[2] * cos + origin[2],
    )


def load(path: str, pack_dir: str = "../x") -> Dump:
    return Dump().load(path, pack_dir)


def look_at(eye: Vec3, target: Vec3) -> Tuple[Vec3, Vec3, Vec3]:
    """A camera basis: right, up and forward, with forward pointing at the target."""
    fx, fy, fz = (target[0] - eye[0], target[1] - eye[1], target[2] - eye[2])
    length = math.sqrt(fx * fx + fy * fy + fz * fz) or 1.0
    forward = (fx / length, fy / length, fz / length)
    # Looking straight down makes the usual up vector parallel to the view, so
    # fall back to the world's -Z, which keeps a top-down view upright.
    world_up = (
        (0.0, 1.0, 0.0)
        if abs(forward[1]) < 0.999
        else (0.0, 0.0, -1.0)
    )
    rx = forward[1] * world_up[2] - forward[2] * world_up[1]
    ry = forward[2] * world_up[0] - forward[0] * world_up[2]
    rz = forward[0] * world_up[1] - forward[1] * world_up[0]
    rl = math.sqrt(rx * rx + ry * ry + rz * rz) or 1.0
    right = (rx / rl, ry / rl, rz / rl)
    up = (
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    )
    return right, up, forward


def _clip(view: Sequence[Vec3], uvs: Sequence[Vec2]) -> Tuple[List[Vec3], List[Vec2]]:
    """Clip a polygon against the near plane, keeping what is in front of it."""
    out_points: List[Vec3] = []
    out_uvs: List[Vec2] = []
    count = len(view)
    for index in range(count):
        a, b = view[index], view[(index + 1) % count]
        ua, ub = uvs[index], uvs[(index + 1) % count]
        if a[2] >= NEAR:
            out_points.append(a)
            out_uvs.append(ua)
        if (a[2] >= NEAR) != (b[2] >= NEAR):
            t = (NEAR - a[2]) / (b[2] - a[2])
            out_points.append(
                (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, NEAR)
            )
            out_uvs.append((ua[0] + (ub[0] - ua[0]) * t, ua[1] + (ub[1] - ua[1]) * t))
    return out_points, out_uvs


def render(
    dump: Dump,
    out_path: str,
    width: int = 960,
    height: int = 540,
    eye: Optional[Vec3] = None,
    target: Optional[Vec3] = None,
    fovy: Optional[float] = None,
    sky: Tuple[int, int, int] = (60, 90, 130),
    textured: bool = True,
    cars: bool = True,
    car_texture: str = "tex/rally.png",
    cull: bool = False,
) -> Dict[str, int]:
    """Draw the dump into a PNG.  Returns how many triangles each texture drew.

    Texture lookups **wrap**, which is what M3G does and what the port's tiled
    atlases stand in for, so this is also the reference the port's tiling can be
    compared against.  ``textured=False`` paints each texture a flat colour
    instead, which is how to read the geometry without the artwork in the way.
    """
    eye = eye or dump.eye or (0.0, 5.0, 10.0)
    target = target or dump.target or (0.0, 0.0, 0.0)
    fovy = fovy or dump.fovy
    right, up, forward = look_at(eye, target)
    focal = height / 2.0 / math.tan(math.radians(fovy) / 2.0)

    colour = bytearray(width * height * 4)
    for index in range(width * height):
        colour[index * 4 + 0] = sky[0]
        colour[index * 4 + 1] = sky[1]
        colour[index * 4 + 2] = sky[2]
        colour[index * 4 + 3] = 255
    depth = [float("inf")] * (width * height)
    drawn: Dict[str, int] = {}

    for path, faces in dump.triangles.items():
        if not cars and path == car_texture:
            continue
        texture = dump.textures.get(path)
        if texture is None:
            continue
        tw, th, rgba = texture
        tint = flat_colour(path)
        for points, uvs in faces:
            view: List[Vec3] = []
            for point in points:
                dx, dy, dz = point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]
                view.append(
                    (
                        dx * right[0] + dy * right[1] + dz * right[2],
                        dx * up[0] + dy * up[1] + dz * up[2],
                        dx * forward[0] + dy * forward[1] + dz * forward[2],
                    )
                )
            if all(point[2] < NEAR for point in view):
                continue
            view, uvs = _clip(view, uvs)
            if len(view) < 3:
                continue
            screen: List[Tuple[float, float, float]] = []
            for x, y, z in view:
                screen.append(
                    (width / 2.0 + x * focal / z, height / 2.0 - y * focal / z, z)
                )
            for corner in range(1, len(screen) - 1):
                face = [screen[0], screen[corner], screen[corner + 1]]
                corner_uvs = [uvs[0], uvs[corner], uvs[corner + 1]]
                # The game draws its world with `PolygonMode.setCulling(160)`,
                # which is CULL_BACK: a face wound the wrong way is not drawn at
                # all.  Turning it on here is how to tell which way the geometry
                # faces - a road whose surface vanishes when seen from above is
                # inside out.
                if cull:
                    area = (face[1][0] - face[0][0]) * (face[2][1] - face[0][1]) - (
                        face[2][0] - face[0][0]
                    ) * (face[1][1] - face[0][1])
                    if area >= 0:
                        continue
                drawn[path] = drawn.get(path, 0) + 1
                _triangle(
                    colour,
                    depth,
                    width,
                    height,
                    face,
                    corner_uvs,
                    (tw, th, rgba) if textured else None,
                    tint,
                )

    png.write_file(out_path, width, height, colour)
    return drawn


def _triangle(
    colour: bytearray,
    depth: List[float],
    width: int,
    height: int,
    face: Sequence[Tuple[float, float, float]],
    uvs: Sequence[Vec2],
    texture: Optional[Tuple[int, int, bytes]],
    tint: Tuple[int, int, int],
) -> None:
    (x0, y0, z0), (x1, y1, z1), (x2, y2, z2) = face
    area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
    if abs(area) < 1e-9:
        return
    left = max(0, int(math.floor(min(x0, x1, x2))))
    right = min(width - 1, int(math.ceil(max(x0, x1, x2))))
    top = max(0, int(math.floor(min(y0, y1, y2))))
    bottom = min(height - 1, int(math.ceil(max(y0, y1, y2))))
    if left > right or top > bottom:
        return
    inv = (1.0 / z0, 1.0 / z1, 1.0 / z2)
    for py in range(top, bottom + 1):
        cy = py + 0.5
        for px in range(left, right + 1):
            cx = px + 0.5
            # Edge functions, divided by the signed area so the winding does not
            # matter: macroquad draws both sides.
            w0 = ((x1 - cx) * (y2 - cy) - (x2 - cx) * (y1 - cy)) / area
            w1 = ((x2 - cx) * (y0 - cy) - (x0 - cx) * (y2 - cy)) / area
            w2 = 1.0 - w0 - w1
            if w0 < 0.0 or w1 < 0.0 or w2 < 0.0:
                continue
            total = w0 * inv[0] + w1 * inv[1] + w2 * inv[2]
            if total <= 0.0:
                continue
            z = 1.0 / total
            index = py * width + px
            if z >= depth[index]:
                continue
            if texture is None:
                pixel = tint
            else:
                tw, th, rgba = texture
                # Interpolating 1/z along with the coordinates is what makes the
                # texture perspective-correct instead of affine.
                u = (w0 * uvs[0][0] * inv[0] + w1 * uvs[1][0] * inv[1] + w2 * uvs[2][0] * inv[2]) * z
                v = (w0 * uvs[0][1] * inv[0] + w1 * uvs[1][1] * inv[1] + w2 * uvs[2][1] * inv[2]) * z
                tx = int((u % 1.0) * tw) % tw
                ty = int((v % 1.0) * th) % th
                offset = (ty * tw + tx) * 4
                pixel = (rgba[offset], rgba[offset + 1], rgba[offset + 2])
            depth[index] = z
            colour[index * 4 + 0] = pixel[0]
            colour[index * 4 + 1] = pixel[1]
            colour[index * 4 + 2] = pixel[2]
            colour[index * 4 + 3] = 255


def flat_colour(path: str) -> Tuple[int, int, int]:
    """A stable colour per texture resource, for drawing without the artwork."""
    digest = 0
    for char in path:
        digest = (digest * 131 + ord(char)) & 0xFFFFFF
    return (60 + digest % 160, 70 + (digest >> 8) % 150, 80 + (digest >> 16) % 140)
