"""Behavioral tests for the pure relative sharpness classifier."""

import importlib.util
import json
from pathlib import Path
import unittest
import warnings

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter


MODULE_PATH = Path(__file__).resolve().parents[1] / "ui_sharpness.py"
classify_pair = None
if MODULE_PATH.is_file():
    spec = importlib.util.spec_from_file_location("ui_sharpness", MODULE_PATH)
    if spec is not None and spec.loader is not None:
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        classify_pair = getattr(module, "classify_pair", None)


GLYPHS = {
    "A": ("01110", "10001", "10001", "11111", "10001", "10001", "10001"),
    "C": ("01111", "10000", "10000", "10000", "10000", "10000", "01111"),
    "D": ("11110", "10001", "10001", "10001", "10001", "10001", "11110"),
    "E": ("11111", "10000", "10000", "11110", "10000", "10000", "11111"),
    "H": ("10001", "10001", "10001", "11111", "10001", "10001", "10001"),
    "I": ("11111", "00100", "00100", "00100", "00100", "00100", "11111"),
    "L": ("10000", "10000", "10000", "10000", "10000", "10000", "11111"),
    "N": ("10001", "11001", "11001", "10101", "10011", "10011", "10001"),
    "O": ("01110", "10001", "10001", "10001", "10001", "10001", "01110"),
    "R": ("11110", "10001", "10001", "11110", "10100", "10010", "10001"),
    "U": ("10001", "10001", "10001", "10001", "10001", "10001", "01110"),
    "Z": ("11111", "00001", "00010", "00100", "01000", "10000", "11111"),
}
WORDS = ("ZURE", "READ", "LINE", "CLEAN", "HERO")
REPLACEMENTS = {"A": "Z", "C": "H", "D": "A", "E": "O", "H": "C", "I": "L", "L": "I", "N": "R", "O": "E", "R": "N", "U": "A", "Z": "U"}


def glyph_image(size: int, variant: int = 0) -> Image.Image:
    """Build deterministic, repeated text-like strokes at 128/256 px scale."""
    scale = size // 64
    image = Image.new("L", (size, size), 28)
    draw = ImageDraw.Draw(image)
    for row, y in enumerate(range(3 * scale, size - 8 * scale, 11 * scale)):
        word = WORDS[(row + variant) % len(WORDS)]
        if variant:
            word = "".join(REPLACEMENTS[char] for char in word)
        word_width = len(word) * 6 * scale - scale
        x = 4 * scale
        while x + word_width <= size - 3 * scale:
            for character_index, character in enumerate(word):
                pattern = GLYPHS[character]
                left = x + character_index * 6 * scale
                for glyph_y, bits in enumerate(pattern):
                    for glyph_x, bit in enumerate(bits):
                        if bit == "1":
                            px = left + glyph_x * scale
                            py = y + glyph_y * scale
                            draw.rectangle(
                                (px, py, px + scale - 1, py + scale - 1),
                                fill=224,
                            )
            x += word_width + 3 * scale
    return image


def affine_contrast(image: Image.Image) -> Image.Image:
    return image.point(lambda value: round(0.45 * value + 15))


def quantization_noise(image: Image.Image) -> Image.Image:
    pixels = np.asarray(image, dtype=np.int16)
    noise = np.random.default_rng(481).integers(-1, 2, pixels.shape)
    return Image.fromarray(np.clip(pixels + noise, 0, 255).astype(np.uint8), mode="L")


class SharpnessClassifierTests(unittest.TestCase):
    def classify(self, reference: Image.Image, current: Image.Image) -> dict:
        self.assertIsNotNone(
            classify_pair,
            "scripts/ui_sharpness.py must export classify_pair(reference, current)",
        )
        return classify_pair(reference, current)

    def assert_json_result(self, result: dict, kind: str) -> None:
        self.assertIsInstance(result, dict)
        self.assertEqual(result.get("kind"), kind, result)
        self.assertIsInstance(result.get("pixel_equal"), bool)
        for field in ("blur_sigma", "contrast_gain", "correlation", "residual"):
            self.assertIn(field, result)
        json.dumps(result)

    def test_gaussian_blur_of_fixed_glyphs_is_a_candidate(self):
        for size in (128, 256):
            with self.subTest(size=size):
                reference = glyph_image(size)
                current = reference.filter(ImageFilter.GaussianBlur(1.2))
                result = self.classify(reference, current)
                self.assert_json_result(result, "blur_candidate")
                self.assertGreaterEqual(result["blur_sigma"], 0.8)
                self.assertGreater(result["correlation"], 0.9)

    def test_affine_contrast_change_is_not_spatial_blur(self):
        reference = glyph_image(256)
        result = self.classify(reference, affine_contrast(reference))
        self.assert_json_result(result, "contrast")
        self.assertAlmostEqual(result["contrast_gain"], 0.45, delta=0.03)

    def test_blur_with_reduced_contrast_remains_a_blur_candidate(self):
        reference = glyph_image(256).point(lambda value: 15 if value < 128 else 100)
        current = reference.filter(ImageFilter.GaussianBlur(1.2)).point(
            lambda value: round(0.45 * value + 15)
        )
        result = self.classify(reference, current)
        self.assert_json_result(result, "blur_candidate")
        self.assertGreaterEqual(result["blur_sigma"], 0.8)
        self.assertGreater(result["correlation"], 0.9)

    def test_identical_already_blurred_reference_is_unchanged(self):
        soft = glyph_image(128).filter(ImageFilter.GaussianBlur(1.2))
        result = self.classify(soft, soft.copy())
        self.assert_json_result(result, "unchanged")
        self.assertTrue(result["pixel_equal"])
        self.assertEqual(result["blur_sigma"], 0.0)

    def test_flat_images_are_unassessable(self):
        flat = Image.new("L", (128, 128), 112)
        result = self.classify(flat, flat.copy())
        self.assert_json_result(result, "unassessable")

    def test_equal_luminance_color_change_is_not_pixel_unchanged(self):
        reference = glyph_image(128).convert("RGB")
        gray = np.asarray(reference.convert("L"))
        colors = np.empty((128, 128, 3), dtype=np.uint8)
        colors[gray <= 128] = (0, 48, 0)
        colors[gray > 128] = (255, 252, 0)
        current = Image.fromarray(colors, mode="RGB")
        self.assertEqual(reference.convert("L").tobytes(), current.convert("L").tobytes())

        result = self.classify(reference, current)
        self.assert_json_result(result, "content_change")
        self.assertFalse(result["pixel_equal"])
        self.assertEqual(result["reason"], "chromatic_change_at_equal_luminance")

    def test_one_pixel_wide_or_high_images_are_unassessable_without_warnings(self):
        for size in ((1, 128), (128, 1)):
            with self.subTest(size=size):
                reference = Image.new("L", size, 24)
                reference.putpixel((0, 0), 240)
                current = reference.copy()
                with warnings.catch_warnings(record=True) as caught:
                    warnings.simplefilter("always")
                    result = self.classify(reference, current)
                self.assert_json_result(result, "unassessable")
                self.assertTrue(result["pixel_equal"])
                self.assertEqual(caught, [])

    def test_one_pixel_shift_and_replaced_glyphs_are_content_changes(self):
        reference = glyph_image(128)
        shifted = ImageChops.offset(reference, 1, 0)
        replaced = glyph_image(128, variant=1)
        for current in (shifted, replaced):
            with self.subTest(current=current):
                result = self.classify(reference, current)
                self.assert_json_result(result, "content_change")

    def test_changed_dimensions_are_geometry_change(self):
        reference = glyph_image(128)
        current = reference.resize((129, 128))
        result = self.classify(reference, current)
        self.assert_json_result(result, "geometry_change")

    def test_one_lsb_quantization_noise_is_not_blur(self):
        reference = glyph_image(256)
        result = self.classify(reference, quantization_noise(reference))
        self.assert_json_result(result, result["kind"])
        self.assertNotEqual(result["kind"], "blur_candidate", result)
        self.assertFalse(result["pixel_equal"])
        self.assertEqual(result["reason"], "affine_equivalent_within_tolerance")

    def test_sharpening_or_recovery_is_not_blur(self):
        reference = glyph_image(256)
        softened = reference.filter(ImageFilter.GaussianBlur(1.2))
        recovered = softened.filter(
            ImageFilter.UnsharpMask(radius=1.2, percent=180, threshold=0)
        )
        for current in (reference, recovered):
            with self.subTest(current=current):
                result = self.classify(softened, current)
                self.assert_json_result(result, result["kind"])
                self.assertNotEqual(result["kind"], "blur_candidate", result)


if __name__ == "__main__":
    unittest.main()
