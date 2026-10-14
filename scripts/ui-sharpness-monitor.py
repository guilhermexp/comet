#!/usr/bin/env python3
"""Observe an existing Zeron window and retain bounded, private PNG evidence."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
from html import escape
import io
import json
import math
import os
from pathlib import Path
import platform
import select
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass

from PIL import Image, ImageFilter, ImageStat


MIB = 1024 * 1024
DEFAULT_DURATION = 300.0
DEFAULT_INTERVAL = 1.0
DEFAULT_BUDGET_MIB = 256.0
MIN_FREE_BYTES = 1024 * MIB
MAX_IMAGE_DIMENSION = 16_384
MAX_IMAGE_PIXELS = 48_000_000
TILE_SIZE = 256
MAX_TILES = 8
MAX_ROIS = 16
MAX_ROI_DIMENSION = 4096
MAX_ROI_PIXELS = 4_000_000
REPORT_RESERVE_BYTES = 16 * 1024
LIMITATIONS = [
    "Classifications are relative to a captured baseline; a baseline that is already soft cannot prove clear rendering.",
    "PNG observations show captured window pixels only; they do not identify GPU, compositor, display, or root cause.",
    "Sampling can miss brief events, and this report is not continuous renderer instrumentation.",
]


class MonitorError(Exception):
    """A requested observation could not safely continue."""


class StorageLimit(MonitorError):
    """The configured output budget has been reached."""


def _positive_float(value: str) -> float:
    try:
        parsed = float(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a finite positive number") from exc
    if not math.isfinite(parsed) or parsed <= 0:
        raise argparse.ArgumentTypeError("must be a finite positive number")
    return parsed


def _budget(value: str) -> float:
    parsed = _positive_float(value)
    if parsed * MIB < REPORT_RESERVE_BYTES * 2:
        raise argparse.ArgumentTypeError("must allow space for the report (at least 0.03125 MiB)")
    if parsed > 4096:
        raise argparse.ArgumentTypeError("must be no more than 4096 MiB")
    return parsed


def _positive_int(value: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a positive integer") from exc
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def _nonnegative_int(value: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a non-negative integer") from exc
    if parsed < 0:
        raise argparse.ArgumentTypeError("must be a non-negative integer")
    return parsed


def _roi(value: str) -> tuple[int, int, int, int]:
    try:
        parts = tuple(int(part, 10) for part in value.split(","))
    except ValueError as exc:
        raise argparse.ArgumentTypeError("ROI must be x,y,width,height using integers") from exc
    if len(parts) != 4 or parts[0] < 0 or parts[1] < 0 or parts[2] <= 0 or parts[3] <= 0:
        raise argparse.ArgumentTypeError("ROI must have non-negative x,y and positive width,height")
    if parts[2] > MAX_ROI_DIMENSION or parts[3] > MAX_ROI_DIMENSION:
        raise argparse.ArgumentTypeError(f"ROI dimensions must be at most {MAX_ROI_DIMENSION}px")
    return parts


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def _json_line(value: dict) -> bytes:
    return (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")


def _write_exclusive(path: Path, data: bytes) -> None:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(fd, "wb", closefd=False) as stream:
            stream.write(data)
            stream.flush()
            os.fsync(fd)
    finally:
        os.close(fd)


class OutputStore:
    def __init__(self, path: Path, max_bytes: int):
        self.path = path.expanduser().absolute()
        self.max_bytes = max_bytes
        self.events: list[dict] = []
        self.source_names: list[str] = []
        self.event_count = 0
        self.sample_count = 0
        self.kind_counts: dict[str, int] = {}
        self.started_at = _timestamp()
        self.mode = "offline"
        self.configuration: dict = {}
        self.window: dict | None = None
        self.error: str | None = None
        self.termination_reason = "running"
        self._obs = None
        self._events = None

    def initialize(self) -> None:
        if self.path.is_symlink():
            raise MonitorError("output path must not be a symlink")
        if self.path.exists():
            if not self.path.is_dir() or any(self.path.iterdir()):
                raise MonitorError("output must be a new or empty directory; refusing to overwrite")
        else:
            self.path.mkdir(mode=0o700, parents=True, exist_ok=False)
        os.chmod(self.path, 0o700)
        evidence = self.path / "evidence"
        evidence.mkdir(mode=0o700, exist_ok=False)
        self._obs = os.fdopen(os.open(self.path / "observations.jsonl", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "ab")
        self._events = os.fdopen(os.open(self.path / "events.jsonl", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "ab")

    def bytes_used(self) -> int:
        total = 0
        for path in self.path.rglob("*"):
            try:
                if path.is_file() and not path.is_symlink():
                    total += path.stat().st_size
            except FileNotFoundError:
                pass
        return total

    def _ensure_budget(self, count: int, replacing: Path | None = None) -> None:
        used = self.bytes_used()
        if replacing is not None and replacing.exists():
            used -= replacing.stat().st_size
        if used + count + REPORT_RESERVE_BYTES > self.max_bytes:
            raise StorageLimit("configured output budget reached")

    def write_observation(self, observation: dict) -> None:
        payload = _json_line(observation)
        self._ensure_budget(len(payload))
        self._obs.write(payload)
        self._obs.flush()
        os.fsync(self._obs.fileno())
        self.sample_count += 1
        kind = str(observation.get("kind", "unassessable"))
        self.kind_counts[kind] = self.kind_counts.get(kind, 0) + 1
        source_name = observation.get("metadata", {}).get("source_name")
        if source_name and len(self.source_names) < 8:
            self.source_names.append(str(source_name)[:120])

    def write_event(self, event: dict) -> None:
        payload = _json_line(event)
        self._ensure_budget(len(payload))
        self._events.write(payload)
        self._events.flush()
        os.fsync(self._events.fileno())
        self.events.append(event)
        self.event_count += 1

    def save_png(self, relative: str, image: Image.Image, *, replace: bool = False) -> str:
        path = self.path / relative
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        buffer = io.BytesIO()
        image.save(buffer, format="PNG", optimize=False)
        payload = buffer.getvalue()
        if path.exists() and (not replace or path.is_symlink()):
            raise MonitorError(f"refusing to replace evidence path: {relative}")
        self._ensure_budget(len(payload), replacing=path if replace else None)
        if replace:
            temp = path.with_name(f".{path.name}.{os.getpid()}.{time.monotonic_ns()}.tmp")
            try:
                _write_exclusive(temp, payload)
                os.replace(temp, path)
            finally:
                try:
                    temp.unlink()
                except FileNotFoundError:
                    pass
        else:
            _write_exclusive(path, payload)
        return relative

    def finalize(self) -> None:
        for handle in (self._obs, self._events):
            if handle is not None and not handle.closed:
                handle.flush()
                os.fsync(handle.fileno())
                handle.close()
        report = {
            "schema_version": 1,
            "mode": self.mode,
            "started_at": self.started_at,
            "finished_at": _timestamp(),
            "samples": self.sample_count,
            "events": self.event_count,
            "classification_counts": self.kind_counts,
            "termination_reason": self.termination_reason,
            "configuration": self.configuration,
            "window": self.window,
            "source_files_preview": self.source_names,
            "limitations": LIMITATIONS,
        }
        if self.error:
            report["error"] = self.error[:1000]
        report_data = (json.dumps(report, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        html_data = _render_html(report, self.events).encode("utf-8")
        # Sampling writes reserve space for these summaries. Finalization consumes
        # that reservation instead of demanding it a second time.
        if self.bytes_used() + len(report_data) + len(html_data) > self.max_bytes:
            raise StorageLimit("configured output budget cannot fit the final report")
        _write_exclusive(self.path / "report.json", report_data)
        _write_exclusive(self.path / "report.html", html_data)


def _render_html(report: dict, events: list[dict]) -> str:
    reason = escape(str(report["termination_reason"]))
    error = f"<p class=error>{escape(str(report['error']))}</p>" if report.get("error") else ""
    kinds = "".join(
        f"<li>{escape(str(kind))}: {int(count)}</li>"
        for kind, count in sorted(report.get("classification_counts", {}).items())
    )
    event_rows = []
    for event in events[:20]:
        links = []
        for label, key in (
            ("before", "before_png"),
            ("after", "after_png"),
            ("window context before", "context_before_png"),
            ("window context after", "context_after_png"),
        ):
            rel = event.get(key)
            if rel:
                safe = escape(str(rel), quote=True)
                links.append(f'<a href="{safe}">{label} PNG</a>')
        event_rows.append(
            "<li>"
            + escape(str(event.get("kind", "event")))
            + " · sample "
            + escape(str(event.get("sample_index", "?")))
            + (" · " + " · ".join(links) if links else "")
            + "</li>"
        )
    limitations = "".join(f"<li>{escape(str(item))}</li>" for item in report.get("limitations", []))
    sources = "".join(f"<li>{escape(str(name))}</li>" for name in report.get("source_files_preview", []))
    return (
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">"
        "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
        "<title>Zeron sharpness observation</title>"
        "<style>body{font:16px system-ui;max-width:70rem;margin:2rem auto;padding:0 1rem;background:#17191c;color:#eee}"
        "a{color:#82c7ff}.error{color:#ffb2a6}code{overflow-wrap:anywhere}</style></head><body>"
        "<h1>Zeron sharpness observation</h1>"
        f"<p>Mode: {escape(str(report['mode']))} · samples: {int(report['samples'])} · termination: {reason}</p>"
        f"{error}<p>Images: <a href=\"baseline.png\">baseline</a> · <a href=\"latest.png\">latest</a></p>"
        "<p>Detailed measurements: <a href=\"observations.jsonl\">observations.jsonl</a> · "
        "<a href=\"events.jsonl\">events.jsonl</a> · <a href=\"report.json\">report.json</a></p>"
        f"<h2>Observed classifications</h2><ul>{kinds or '<li>No samples</li>'}</ul>"
        f"<h2>PNG sources</h2><ul>{sources or '<li>Live window capture</li>'}</ul>"
        f"<h2>Events ({int(report['events'])})</h2><ul>{''.join(event_rows) or '<li>None recorded</li>'}</ul>"
        f"<h2>Limitations</h2><ul>{limitations}</ul></body></html>\n"
    )


def _load_png(path: Path) -> Image.Image:
    try:
        with path.open("rb") as handle:
            if handle.read(8) != b"\x89PNG\r\n\x1a\n":
                raise MonitorError(f"not a PNG file: {path.name}")
        with Image.open(path) as image:
            if image.format != "PNG" or getattr(image, "n_frames", 1) != 1:
                raise MonitorError(f"unsupported image (expected a single-frame PNG): {path.name}")
            width, height = image.size
            if width <= 0 or height <= 0 or width > MAX_IMAGE_DIMENSION or height > MAX_IMAGE_DIMENSION:
                raise MonitorError(f"image dimensions exceed bounds: {path.name}")
            if width * height > MAX_IMAGE_PIXELS:
                raise MonitorError(f"image pixel count exceeds bound: {path.name}")
            image.verify()
        with Image.open(path) as image:
            image.load()
            return image.convert("RGBA")
    except MonitorError:
        raise
    except Exception as exc:
        raise MonitorError(f"invalid PNG {path.name}: {exc}") from exc


def _edge_tiles(image: Image.Image) -> list[tuple[str, tuple[int, int, int, int]]]:
    gray = image.convert("L")
    width, height = gray.size
    ranked = []
    for top in range(0, height, TILE_SIZE):
        for left in range(0, width, TILE_SIZE):
            right = min(width, left + TILE_SIZE)
            bottom = min(height, top + TILE_SIZE)
            if right - left < 24 or bottom - top < 16:
                continue
            box = (left, top, right, bottom)
            edges = gray.crop(box).filter(ImageFilter.FIND_EDGES)
            energy = ImageStat.Stat(edges).mean[0]
            if energy >= 1.0:
                ranked.append((energy, box))
    ranked.sort(key=lambda item: (-item[0], item[1][1], item[1][0]))
    # Keep the bounded sample spatially representative. One edge-bearing tile
    # per 4x2 screen cell prevents a dense transcript in the center from
    # displacing sparse but useful side panels such as Workers or Details.
    cells: dict[tuple[int, int], list[tuple[float, tuple[int, int, int, int]]]] = {}
    for item in ranked:
        _, (left, top, right, bottom) = item
        col = min(3, int(((left + right) / 2) * 4 / width))
        row = min(1, int(((top + bottom) / 2) * 2 / height))
        cells.setdefault((col, row), []).append(item)
    selected = [items[0] for _, items in sorted(cells.items())]
    selected_boxes = {box for _, box in selected}
    if len(selected) < MAX_TILES:
        selected.extend(
            item for item in ranked
            if item[1] not in selected_boxes
        )
    tiles = sorted(selected[:MAX_TILES], key=lambda item: (item[1][1], item[1][0]))
    return [
        (f"tile-{index:02d}-x{box[0]}-y{box[1]}", box)
        for index, (_, box) in enumerate(tiles)
    ]


def _classify(reference: Image.Image, current: Image.Image) -> dict:
    try:
        from ui_sharpness import classify_pair
    except ImportError as exc:
        raise MonitorError("scripts/ui_sharpness.py detector is unavailable") from exc
    try:
        result = classify_pair(reference, current)
    except Exception as exc:
        raise MonitorError(f"sharpness classifier failed: {exc}") from exc
    if not isinstance(result, dict) or result.get("kind") not in {
        "unchanged", "contrast", "content_change", "geometry_change", "unassessable", "blur_candidate"
    }:
        raise MonitorError("sharpness classifier returned an invalid result")
    return result


def _json_safe_metrics(result: dict) -> dict:
    safe = {"kind": result["kind"]}
    numeric = (
        "blur_sigma", "contrast_gain", "correlation", "residual", "reverse_sigma",
        "spatial_improvement", "relative_improvement", "affine_residual",
    )
    for key in numeric:
        if key not in result:
            continue
        value = result.get(key)
        try:
            number = float(value)
        except (TypeError, ValueError):
            safe[key] = None
            continue
        safe[key] = number if math.isfinite(number) else None
    for key in ("reason", "spatial_direction"):
        value = result.get(key)
        if isinstance(value, str):
            safe[key] = value[:160]
    if isinstance(result.get("pixel_equal"), bool):
        safe["pixel_equal"] = result["pixel_equal"]
    for key in ("reference_size", "current_size"):
        value = result.get(key)
        if isinstance(value, (list, tuple)) and len(value) == 2:
            try:
                safe[key] = [int(value[0]), int(value[1])]
            except (TypeError, ValueError):
                pass
    return safe


def _exact_pixel_metrics(reference: Image.Image, current: Image.Image) -> dict:
    def digest(image: Image.Image) -> str:
        prefix = f"{image.mode}:{image.width}x{image.height}:".encode("ascii")
        return hashlib.sha256(prefix + image.tobytes()).hexdigest()

    reference_hash = digest(reference)
    current_hash = digest(current)
    return {
        "pixel_equal": reference.size == current.size and reference_hash == current_hash,
        "reference_sha256": reference_hash,
        "current_sha256": current_hash,
    }


@dataclass(frozen=True)
class WindowTarget:
    window_id: int
    pid: int
    owner: str
    executable: str
    bounds: tuple[float, float, float, float]

    def as_json(self, image_size: tuple[int, int] | None = None) -> dict:
        result = {
            "window_id": self.window_id,
            "pid": self.pid,
            "owner": self.owner,
            "executable": self.executable,
            "bounds_points": {"x": self.bounds[0], "y": self.bounds[1], "width": self.bounds[2], "height": self.bounds[3]},
        }
        if image_size and self.bounds[2] > 0 and self.bounds[3] > 0:
            result["png_size_pixels"] = {"width": image_size[0], "height": image_size[1]}
            result["observed_png_pixels_per_window_point"] = {
                "x": image_size[0] / self.bounds[2],
                "y": image_size[1] / self.bounds[3],
            }
        return result


class NativeCaptureProvider:
    """CoreGraphics window enumeration plus window-ID-only PNG capture."""

    def __init__(self):
        if platform.system() != "Darwin":
            raise MonitorError("live window capture is supported only on macOS")
        import ctypes

        self.ctypes = ctypes
        try:
            self.cg = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
            self.cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
        except OSError as exc:
            raise MonitorError(f"could not load macOS CoreGraphics: {exc}") from exc
        self._bind_core_foundation()

    def _bind_core_foundation(self) -> None:
        c = self.ctypes
        self.cg.CGWindowListCopyWindowInfo.argtypes = [c.c_uint32, c.c_uint32]
        self.cg.CGWindowListCopyWindowInfo.restype = c.c_void_p
        self.cg.CGPreflightScreenCaptureAccess.argtypes = []
        self.cg.CGPreflightScreenCaptureAccess.restype = c.c_bool
        self.cf.CFArrayGetCount.argtypes = [c.c_void_p]
        self.cf.CFArrayGetCount.restype = c.c_long
        self.cf.CFArrayGetValueAtIndex.argtypes = [c.c_void_p, c.c_long]
        self.cf.CFArrayGetValueAtIndex.restype = c.c_void_p
        self.cf.CFDictionaryGetValue.argtypes = [c.c_void_p, c.c_void_p]
        self.cf.CFDictionaryGetValue.restype = c.c_void_p
        self.cf.CFStringCreateWithCString.argtypes = [c.c_void_p, c.c_char_p, c.c_uint32]
        self.cf.CFStringCreateWithCString.restype = c.c_void_p
        self.cf.CFStringGetLength.argtypes = [c.c_void_p]
        self.cf.CFStringGetLength.restype = c.c_long
        self.cf.CFStringGetCString.argtypes = [c.c_void_p, c.c_void_p, c.c_long, c.c_uint32]
        self.cf.CFStringGetCString.restype = c.c_bool
        self.cf.CFNumberGetValue.argtypes = [c.c_void_p, c.c_int, c.c_void_p]
        self.cf.CFNumberGetValue.restype = c.c_bool
        self.cf.CFRelease.argtypes = [c.c_void_p]
        self.cf.CFRelease.restype = None

    def preflight(self) -> None:
        try:
            allowed = bool(self.cg.CGPreflightScreenCaptureAccess())
        except AttributeError as exc:
            raise MonitorError("macOS screen-capture preflight is unavailable; refusing to trigger a permission prompt") from exc
        if not allowed:
            raise MonitorError("macOS screen recording permission is not already granted; no prompt was requested")

    def _key(self, value: str):
        return self.cf.CFStringCreateWithCString(None, value.encode("utf-8"), 0x08000100)

    def _value(self, dictionary, key: str):
        key_ref = self._key(key)
        try:
            return self.cf.CFDictionaryGetValue(dictionary, key_ref)
        finally:
            self.cf.CFRelease(key_ref)

    def _string(self, ref) -> str:
        if not ref:
            return ""
        c = self.ctypes
        capacity = max(256, int(self.cf.CFStringGetLength(ref) * 4 + 8))
        buffer = c.create_string_buffer(capacity)
        if not self.cf.CFStringGetCString(ref, buffer, capacity, 0x08000100):
            return ""
        return buffer.value.decode("utf-8", errors="replace")

    def _number(self, ref, *, floating: bool = False) -> float | int:
        if not ref:
            return 0.0 if floating else 0
        if floating:
            value = self.ctypes.c_double()
            self.cf.CFNumberGetValue(ref, 6, self.ctypes.byref(value))
            return float(value.value)
        value = self.ctypes.c_longlong()
        self.cf.CFNumberGetValue(ref, 4, self.ctypes.byref(value))
        return int(value.value)

    def _bounds(self, ref) -> tuple[float, float, float, float]:
        return tuple(float(self._number(self._value(ref, key), floating=True)) for key in ("X", "Y", "Width", "Height"))

    def _executable(self, pid: int) -> str:
        try:
            libproc = self.ctypes.CDLL("/usr/lib/libproc.dylib")
            proc_pidpath = libproc.proc_pidpath
            proc_pidpath.argtypes = [self.ctypes.c_int, self.ctypes.c_void_p, self.ctypes.c_uint32]
            proc_pidpath.restype = self.ctypes.c_int
            buffer = self.ctypes.create_string_buffer(4096)
            if proc_pidpath(pid, buffer, len(buffer)) > 0:
                return os.fsdecode(buffer.value)
        except (OSError, AttributeError):
            pass
        try:
            result = subprocess.run(["/bin/ps", "-p", str(pid), "-o", "comm="], capture_output=True, text=True, timeout=2, check=True)
            return result.stdout.strip()
        except (OSError, subprocess.SubprocessError) as exc:
            raise MonitorError(f"could not verify process {pid}: {exc}") from exc

    @staticmethod
    def _is_zeron(owner: str, executable: str) -> bool:
        owner_ok = owner.strip().casefold() in {"zeron", "zeron dev"}
        executable_name = Path(executable).name.casefold()
        return owner_ok and executable_name == "zeron"

    def _windows(self) -> list[WindowTarget]:
        c = self.ctypes
        rows = self.cg.CGWindowListCopyWindowInfo(1 | 16, 0)
        if not rows:
            raise MonitorError("CoreGraphics returned no on-screen windows")
        targets = []
        try:
            count = self.cf.CFArrayGetCount(rows)
            for index in range(count):
                row = self.cf.CFArrayGetValueAtIndex(rows, index)
                if not row:
                    continue
                layer = self._number(self._value(row, "kCGWindowLayer"))
                if layer != 0:
                    continue
                window_id = self._number(self._value(row, "kCGWindowNumber"))
                pid = self._number(self._value(row, "kCGWindowOwnerPID"))
                owner = self._string(self._value(row, "kCGWindowOwnerName"))
                bounds_ref = self._value(row, "kCGWindowBounds")
                if not window_id or not pid or not owner or not bounds_ref:
                    continue
                bounds = self._bounds(bounds_ref)
                if not all(math.isfinite(value) for value in bounds) or bounds[2] <= 0 or bounds[3] <= 0:
                    continue
                if owner.strip().casefold() not in {"zeron", "zeron dev"}:
                    continue
                executable = self._executable(int(pid))
                if not self._is_zeron(owner, executable):
                    continue
                if bounds[2] < 320 or bounds[3] < 200:
                    continue
                targets.append(WindowTarget(int(window_id), int(pid), owner, executable, bounds))
        finally:
            self.cf.CFRelease(rows)
        return targets

    def discover(self, requested_window_id: int | None = None) -> WindowTarget:
        targets = self._windows()
        if requested_window_id is not None:
            matching = [target for target in targets if target.window_id == requested_window_id]
            if len(matching) != 1:
                raise MonitorError(f"window {requested_window_id} is missing or does not belong to Zeron")
            return matching[0]
        if not targets:
            raise MonitorError("no visible Zeron window of at least 320x200 points was found")
        if len(targets) != 1:
            raise MonitorError(f"found {len(targets)} Zeron windows; specify one with --window-id")
        return targets[0]

    def validate(self, expected: WindowTarget) -> WindowTarget:
        targets = self._windows()
        matches = [target for target in targets if target.window_id == expected.window_id]
        if len(matches) != 1:
            raise MonitorError("selected Zeron window disappeared or changed owner; stopping without selecting another window")
        actual = matches[0]
        if actual.pid != expected.pid or actual.owner != expected.owner or actual.executable != expected.executable:
            raise MonitorError("selected window owner or PID changed; stopping without selecting another window")
        return actual

    def capture(self, expected: WindowTarget) -> tuple[Image.Image, WindowTarget]:
        target = self.validate(expected)
        with tempfile.TemporaryDirectory(prefix="zeron-sharpness-") as tempdir:
            png_path = Path(tempdir) / "window.png"
            try:
                result = subprocess.run(
                    ["/usr/sbin/screencapture", "-x", "-o", "-l", str(target.window_id), str(png_path)],
                    capture_output=True, text=True, timeout=8, check=False,
                )
            except subprocess.TimeoutExpired as exc:
                raise MonitorError("screencapture timed out") from exc
            except OSError as exc:
                raise MonitorError(f"could not run macOS screencapture: {exc}") from exc
            if result.returncode != 0 or not png_path.is_file():
                raise MonitorError(f"screencapture failed: {result.stderr.strip()[:500] or result.returncode}")
            image = _load_png(png_path)
        target = self.validate(expected)
        return image, target


class Monitor:
    def __init__(self, output: OutputStore, rois: list[tuple[int, int, int, int]]):
        self.output = output
        self.rois = rois
        self.reference: Image.Image | None = None
        self.latest: Image.Image | None = None
        self.previous: Image.Image | None = None
        self.regions: list[tuple[str, tuple[int, int, int, int]]] = []
        self.region_baselines: dict[str, Image.Image] = {}
        self.region_versions: dict[str, int] = {}
        self.reported_candidates: set[str] = set()
        self.sample_index = -1
        self.baseline_version = 0
        self._event_index = 0

    def _regions_for(self, image: Image.Image) -> list[tuple[str, tuple[int, int, int, int]]]:
        width, height = image.size
        if self.rois:
            if len(self.rois) > MAX_ROIS:
                raise MonitorError(f"at most {MAX_ROIS} ROI regions are allowed")
            total = 0
            regions = []
            for index, (x, y, rw, rh) in enumerate(self.rois):
                if x + rw > width or y + rh > height:
                    raise MonitorError(f"ROI {index} lies outside the {width}x{height}px frame")
                total += rw * rh
                regions.append((f"roi-{index:02d}", (x, y, x + rw, y + rh)))
            if total > MAX_ROI_PIXELS:
                raise MonitorError(f"combined ROI area exceeds {MAX_ROI_PIXELS} pixels")
            return regions
        regions = _edge_tiles(image)
        return regions or [("whole-frame", (0, 0, width, height))]

    def _event(self, kind: str, sample_index: int, before: Image.Image, after: Image.Image, **fields) -> None:
        self._event_index += 1
        stem = f"evidence/event-{self._event_index:04d}-{kind}"
        before_path = self.output.save_png(f"{stem}-before.png", before)
        after_path = self.output.save_png(f"{stem}-after.png", after)
        row = {
            "kind": kind,
            "sample_index": sample_index,
            "timestamp": _timestamp(),
            "before_png": before_path,
            "after_png": after_path,
            **fields,
        }
        self.output.write_event(row)

    def _save_window_context(self, sample_index: int, before: Image.Image, after: Image.Image) -> tuple[str, str]:
        stem = f"evidence/sample-{sample_index:06d}-window-context"
        return (
            self.output.save_png(f"{stem}-before.png", before),
            self.output.save_png(f"{stem}-after.png", after),
        )

    def mark(self, sample_index: int | None = None) -> None:
        if self.latest is None:
            raise MonitorError("cannot mark an incident before the first sample")
        index = self.sample_index if sample_index is None else sample_index
        before = self.previous or self.reference or self.latest
        self._event("manual_mark", index, before, self.latest, source="keyboard" if sample_index is None else "offline")

    def observe(self, image: Image.Image, metadata: dict, *, timestamp: str | None = None) -> str:
        if image.width > MAX_IMAGE_DIMENSION or image.height > MAX_IMAGE_DIMENSION or image.width * image.height > MAX_IMAGE_PIXELS:
            raise MonitorError("captured image dimensions exceed configured bounds")
        now = timestamp or _timestamp()
        prior_latest = self.latest
        self.sample_index += 1
        image = image.convert("RGBA")
        comparisons = []
        candidate_names = set()
        region_rebaselines: list[tuple[str, str, Image.Image]] = []
        global_rebaseline_reason = None
        if self.reference is None:
            self.reference = image.copy()
            self.regions = self._regions_for(image)
            self.region_baselines = {name: image.crop(box) for name, box in self.regions}
            self.region_versions = {name: 0 for name, _ in self.regions}
            kind = "baseline"
        elif self.reference.size != image.size:
            result = _classify(self.reference, image)
            metrics = _json_safe_metrics(result)
            metrics.update(_exact_pixel_metrics(self.reference, image))
            comparisons.append({"region": "whole-frame", **metrics})
            kind = "geometry_change"
            global_rebaseline_reason = "geometry_change"
        else:
            for name, box in self.regions:
                old = self.region_baselines[name]
                new = image.crop(box)
                result = _classify(old, new)
                metrics = _json_safe_metrics(result)
                metrics.update(_exact_pixel_metrics(old, new))
                comparisons.append({
                    "region": name,
                    "box_pixels": [box[0], box[1], box[2] - box[0], box[3] - box[1]],
                    "baseline_version": self.region_versions[name],
                    **metrics,
                })
                if result["kind"] in {"content_change", "geometry_change"}:
                    region_rebaselines.append((name, result["kind"], new.copy()))
                elif result["kind"] == "blur_candidate":
                    candidate_names.add(name)
            if candidate_names:
                kind = "blur_candidate"
            elif any(reason == "geometry_change" for _, reason, _ in region_rebaselines):
                kind = "geometry_change"
            elif region_rebaselines:
                kind = "content_change"
            elif any(row["kind"] == "contrast" for row in comparisons):
                kind = "contrast"
            elif any(row["kind"] == "unassessable" for row in comparisons):
                kind = "unassessable"
            else:
                kind = "unchanged"
        row = {
            "sample_index": self.sample_index,
            "timestamp": now,
            "kind": kind,
            "image_size_pixels": {"width": image.width, "height": image.height},
            "metadata": metadata,
            "baseline_version": self.baseline_version,
            "region_baseline_versions": dict(self.region_versions),
            "region_coverage": [
                {"name": name, "box_pixels": [box[0], box[1], box[2] - box[0], box[3] - box[1]]}
                for name, box in (
                    self._regions_for(image) if global_rebaseline_reason else self.regions
                )
            ],
            "comparisons": comparisons,
        }
        if self.regions:
            self.output.configuration["region_coverage"] = row["region_coverage"]
            self.output.configuration.setdefault(
                "coverage_strategy",
                "explicit ROIs" if self.rois else f"up to {MAX_TILES} edge-bearing 256px tiles distributed across a 4x2 screen grid",
            )
        self.output.write_observation(row)
        self.output.save_png("latest.png", image, replace=self.output.sample_count > 1)
        if self.baseline_version == 0 and self.output.sample_count == 1:
            self.output.save_png("baseline.png", image)
        if global_rebaseline_reason:
            old_reference = self.reference
            self._event("rebaseline", self.sample_index, old_reference, image, reason=global_rebaseline_reason, scope="all_regions")
            self.reference = image.copy()
            self.baseline_version += 1
            self.regions = self._regions_for(image)
            self.region_baselines = {name: image.crop(box) for name, box in self.regions}
            self.region_versions = {name: self.baseline_version for name, _ in self.regions}
            self.reported_candidates.clear()
            self.output.save_png("baseline.png", image, replace=True)
        else:
            if region_rebaselines:
                reasons = {name: reason for name, reason, _ in region_rebaselines}
                prior = prior_latest or self.reference
                self._event(
                    "rebaseline", self.sample_index, prior, image,
                    reason="content_change" if "content_change" in reasons.values() else "geometry_change",
                    scope="regions", regions=sorted(reasons), region_reasons=reasons,
                )
                for name, _, new_region_baseline in region_rebaselines:
                    self.region_baselines[name] = new_region_baseline
                    self.region_versions[name] = self.region_versions.get(name, 0) + 1
                    self.reported_candidates.discard(name)
            new_candidates = candidate_names - self.reported_candidates
            if new_candidates:
                context_before, context_after = self._save_window_context(
                    self.sample_index, prior_latest or self.reference, image,
                )
                boxes = dict(self.regions)
                measurements = {item["region"]: item for item in comparisons}
                for name in sorted(new_candidates):
                    box = boxes[name]
                    self._event(
                        "blur_candidate", self.sample_index,
                        self.region_baselines[name], image.crop(box),
                        evidence_scope="region",
                        box_pixels=[box[0], box[1], box[2] - box[0], box[3] - box[1]],
                        region=name,
                        baseline_version=self.region_versions[name],
                        regions=[name],
                        measurement=measurements[name],
                        measurements=[measurements[name]],
                        context_before_png=context_before,
                        context_after_png=context_after,
                    )
                self.reported_candidates.update(new_candidates)
            self.reported_candidates.difference_update(self.reported_candidates - candidate_names - {name for name, _, _ in region_rebaselines})
        self.previous = prior_latest
        self.latest = image.copy()
        return kind


def _free_disk_check(path: Path) -> None:
    try:
        free = shutil.disk_usage(path).free
    except OSError as exc:
        raise MonitorError(f"could not check free disk space: {exc}") from exc
    if free < MIN_FREE_BYTES:
        raise MonitorError("less than 1 GiB free; refusing to begin or continue capture")


def _offline_files(directory: Path) -> list[Path]:
    if not directory.is_dir():
        raise MonitorError("--analyze must name an existing directory")
    return sorted(
        (path for path in directory.iterdir() if path.suffix.casefold() == ".png" and path.is_file() and not path.is_symlink()),
        key=lambda path: path.name.casefold(),
    )


def _run_offline(args, monitor: Monitor) -> None:
    source = Path(args.analyze).expanduser()
    paths = _offline_files(source)
    if not paths:
        raise MonitorError("--analyze directory contains no direct-child PNG files")
    max_samples = max(1, math.ceil(args.duration / args.interval))
    limit_hit = len(paths) > max_samples
    paths = paths[:max_samples]
    marks = set(args.mark_at)
    for index, path in enumerate(paths):
        _free_disk_check(monitor.output.path)
        image = _load_png(path)
        monitor.observe(image, {"source_name": path.name, "source": "offline"})
        if index in marks:
            monitor.mark(index)
        if limit_hit and index + 1 >= max_samples:
            monitor.output.termination_reason = "duration_limit"
    invalid_marks = sorted(mark for mark in marks if mark >= len(paths))
    if invalid_marks:
        raise MonitorError(f"--mark-at index outside available samples: {invalid_marks}")
    if not limit_hit:
        monitor.output.termination_reason = "completed"


def _poll_enter() -> bool:
    if not sys.stdin.isatty():
        return False
    try:
        ready, _, _ = select.select([sys.stdin], [], [], 0)
        if ready:
            sys.stdin.readline()
            return True
    except (OSError, ValueError):
        return False
    return False


def _run_live(args, monitor: Monitor, provider=None) -> None:
    provider = provider or NativeCaptureProvider()
    provider.preflight()
    target = provider.discover(args.window_id)
    monitor.output.window = target.as_json()
    monitor.output.configuration["window_id"] = target.window_id
    print(f"Observing existing Zeron window {target.window_id} (pid {target.pid}, {target.owner}); Enter marks an incident.", flush=True)
    start = time.monotonic()
    deadline = start + args.duration
    next_sample = start
    while time.monotonic() < deadline:
        wait_until = min(next_sample, deadline)
        wake_at = time.monotonic() + max(0.0, wait_until - time.monotonic())
        while time.monotonic() < wake_at:
            if _poll_enter():
                monitor.mark()
                print(f"manual mark recorded at sample {monitor.sample_index}", flush=True)
            time.sleep(min(0.1, max(0.0, wake_at - time.monotonic())))
        if time.monotonic() >= deadline:
            break
        _free_disk_check(monitor.output.path)
        _free_disk_check(Path(tempfile.gettempdir()))
        target = provider.validate(target)
        image, target = provider.capture(target)
        metadata = {
            "source": "CoreGraphics+screencapture PNG",
            "window": target.as_json(image.size),
        }
        monitor.output.window = target.as_json(image.size)
        kind = monitor.observe(image, metadata)
        if _poll_enter():
            monitor.mark()
            print(f"manual mark recorded at sample {monitor.sample_index}", flush=True)
        if monitor.sample_index == 0:
            if args.roi:
                sampling = "explicit ROI regions"
            else:
                sampling = "edge-bearing 256px regions across the window (not a full-window sharpness guarantee)"
            print(f"Sampling {len(monitor.regions)} {sampling}: {monitor.output.configuration['region_coverage']}", flush=True)
        status = "blur candidate — inspect retained PNGs" if kind == "blur_candidate" else "relative observation"
        print(f"sample {monitor.sample_index}: {kind} ({status}; no absolute sharpness verdict)", flush=True)
        next_sample += args.interval
        if next_sample <= time.monotonic():
            next_sample = time.monotonic()
    monitor.output.termination_reason = "duration_elapsed"


def _default_output() -> Path:
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    return Path(tempfile.gettempdir()) / f"zeron-sharpness-{stamp}-{os.getpid()}"


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Observe an existing Zeron window, or replay a directory of PNGs offline.",
        epilog="Live: run with no arguments while Zeron is open; press Enter to mark an incident. Reports are relative observations, never an all-clear verdict.",
    )
    parser.add_argument("--analyze", metavar="DIR", help="replay direct-child PNG files offline (does not access macOS window APIs)")
    parser.add_argument("--out", type=Path, default=None, help="new or empty private report directory (default: unique system-temp directory)")
    parser.add_argument("--mark-at", type=_nonnegative_int, action="append", default=[], metavar="INDEX", help="mark an offline sample index; repeatable")
    parser.add_argument("--duration", type=_positive_float, default=DEFAULT_DURATION, help="maximum observation duration in seconds (default: 300)")
    parser.add_argument("--interval", type=_positive_float, default=DEFAULT_INTERVAL, help="seconds between live samples; offline sample cap uses duration/interval (default: 1)")
    parser.add_argument("--window-id", type=_positive_int, help="select this existing Zeron window ID (required if more than one eligible window exists)")
    parser.add_argument("--roi", type=_roi, action="append", default=[], metavar="X,Y,W,H", help="compare a native-pixel crop; repeatable, at most 16")
    parser.add_argument("--max-mib", type=_budget, default=DEFAULT_BUDGET_MIB, metavar="MIB", help="maximum report and evidence size (default: 256 MiB)")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.duration > 86_400:
        print("error: --duration must not exceed 86400 seconds", file=sys.stderr)
        return 2
    if args.interval < 0.05:
        print("error: --interval must be at least 0.05 seconds", file=sys.stderr)
        return 2
    if len(args.roi) > MAX_ROIS:
        print(f"error: at most {MAX_ROIS} --roi values are allowed", file=sys.stderr)
        return 2
    output_path = args.out or _default_output()
    output = OutputStore(output_path, int(args.max_mib * MIB))
    try:
        output.initialize()
    except (OSError, MonitorError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    output.mode = "offline" if args.analyze else "live"
    output.configuration = {
        "duration_seconds": args.duration,
        "interval_seconds": args.interval,
        "max_output_mib": args.max_mib,
        "rois_native_pixels": [list(roi) for roi in args.roi],
        "window_id_requested": args.window_id,
        "offline_mark_indices": args.mark_at[:20] if args.analyze else [],
        "offline_mark_count": len(args.mark_at) if args.analyze else 0,
        "tile_size_pixels": TILE_SIZE if not args.roi else None,
        "tile_limit": MAX_TILES if not args.roi else None,
    }
    monitor = Monitor(output, args.roi)
    try:
        _free_disk_check(output.path)
        _free_disk_check(Path(tempfile.gettempdir()))
        if args.analyze:
            _run_offline(args, monitor)
        else:
            _run_live(args, monitor)
        if output.termination_reason == "running":
            output.termination_reason = "completed"
    except StorageLimit as exc:
        output.termination_reason = "storage_limit"
        output.error = str(exc)
    except KeyboardInterrupt:
        output.termination_reason = "interrupted"
    except (MonitorError, OSError, subprocess.SubprocessError) as exc:
        output.termination_reason = "error"
        output.error = str(exc)
        print(f"error: {exc}", file=sys.stderr)
    except Exception as exc:
        output.termination_reason = "error"
        output.error = f"unexpected {type(exc).__name__}: {exc}"
        print(f"error: {output.error}", file=sys.stderr)
    try:
        output.finalize()
    except (OSError, MonitorError, StorageLimit) as exc:
        print(f"error: could not finalize report: {exc}", file=sys.stderr)
        return 1
    print(f"Report: {output.path / 'report.html'} (termination: {output.termination_reason})", flush=True)
    if output.termination_reason == "interrupted":
        return 130
    if output.termination_reason == "error":
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
