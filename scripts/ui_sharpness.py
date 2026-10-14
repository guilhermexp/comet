"""Conservative, relative sharpness classification for two PNG-like images.

This detector compares the current capture with a fixed reference; it cannot
prove that the reference is sharp or that a display/compositor caused a change.
Affine intensity is fitted before testing Gaussian smoothing, so reduced
contrast alone is not treated as blur. A blur candidate requires a best-fit
reference blur radius of at least 0.75 px, correlation of at least 0.90, and a
residual reduction of at least max(0.004, 25% of the target's normalized RMS
signal) and 50%.

The finite 0.5..3.0 px search on a 0.25 px grid is intentionally conservative
and targets small native text changes. It can miss larger blur, non-Gaussian
effects, localized changes, or changes hidden by an already-soft reference.
"""

from __future__ import annotations

import math

import numpy as np
from PIL import Image, ImageFilter


_BLUR_RADII = tuple(round(value * 0.25, 2) for value in range(2, 13))
_MIN_EDGE_RMS = 1.0
_MIN_TEXTURE_STDDEV = 2.0
_UNCHANGED_RESIDUAL = 0.006
_BLUR_MIN_RADIUS = 0.75
_BLUR_MIN_CORRELATION = 0.90
_BLUR_MIN_ABSOLUTE_IMPROVEMENT = 0.004
_BLUR_MIN_SIGNAL_FRACTION = 0.25
_BLUR_MIN_RELATIVE_IMPROVEMENT = 0.50


def _result(
    kind: str,
    *,
    pixel_equal: bool = False,
    blur_sigma: float | None = 0.0,
    contrast_gain: float | None = None,
    correlation: float | None = None,
    residual: float | None = None,
    **details: object,
) -> dict[str, object]:
    result: dict[str, object] = {
        "kind": kind,
        "pixel_equal": pixel_equal,
        "blur_sigma": blur_sigma,
        "contrast_gain": contrast_gain,
        "correlation": correlation,
        "residual": residual,
    }
    result.update(details)
    return result


def _gray_pixels(image: Image.Image) -> np.ndarray:
    return np.asarray(image.convert("L"), dtype=np.float64)


def _same_color_pixels(first: Image.Image, second: Image.Image) -> bool:
    return first.convert("RGBA").tobytes() == second.convert("RGBA").tobytes()


def _edge_rms(pixels: np.ndarray) -> float:
    if pixels.shape[0] < 2 or pixels.shape[1] < 2:
        return 0.0
    dx = np.diff(pixels, axis=1)
    dy = np.diff(pixels, axis=0)
    energy = (float(np.mean(dx * dx)) + float(np.mean(dy * dy))) / 2.0
    return math.sqrt(energy)


def _fit_affine(model: np.ndarray, target: np.ndarray) -> dict[str, float]:
    source = model.reshape(-1)
    observed = target.reshape(-1)
    source_mean = float(np.mean(source))
    observed_mean = float(np.mean(observed))
    centered_source = source - source_mean
    centered_observed = observed - observed_mean
    source_variance = float(np.mean(centered_source * centered_source))
    observed_variance = float(np.mean(centered_observed * centered_observed))
    if source_variance <= 1e-12:
        gain = 0.0
    else:
        gain = float(np.mean(centered_source * centered_observed) / source_variance)
    offset = observed_mean - gain * source_mean
    residual_pixels = gain * source + offset - observed
    residual = float(np.sqrt(np.mean(residual_pixels * residual_pixels)) / 255.0)
    denominator = math.sqrt(source_variance * observed_variance)
    correlation = (
        float(np.mean(centered_source * centered_observed) / denominator)
        if denominator > 1e-12
        else 1.0
    )
    return {
        "gain": gain,
        "offset": offset,
        "correlation": correlation,
        "residual": residual,
    }


def _best_gaussian_fit(
    model_image: Image.Image,
    target: np.ndarray,
) -> dict[str, float]:
    best: dict[str, float] | None = None
    for sigma in _BLUR_RADII:
        softened = model_image.filter(ImageFilter.GaussianBlur(sigma))
        fit = _fit_affine(_gray_pixels(softened), target)
        fit["sigma"] = sigma
        if best is None or fit["residual"] < best["residual"]:
            best = fit
    assert best is not None
    return best


def _spatial_improvement(
    affine_fit: dict[str, float],
    gaussian_fit: dict[str, float],
) -> tuple[float, float]:
    absolute = affine_fit["residual"] - gaussian_fit["residual"]
    relative = absolute / max(affine_fit["residual"], 1e-12)
    return absolute, relative


def _is_blur_evidence(
    affine_fit: dict[str, float],
    gaussian_fit: dict[str, float],
    target_signal_rms: float,
) -> bool:
    absolute, relative = _spatial_improvement(affine_fit, gaussian_fit)
    minimum_absolute = max(
        _BLUR_MIN_ABSOLUTE_IMPROVEMENT,
        _BLUR_MIN_SIGNAL_FRACTION * target_signal_rms,
    )
    return (
        gaussian_fit["sigma"] >= _BLUR_MIN_RADIUS
        and gaussian_fit["correlation"] >= _BLUR_MIN_CORRELATION
        and absolute >= minimum_absolute
        and relative >= _BLUR_MIN_RELATIVE_IMPROVEMENT
    )


def classify_pair(reference: Image.Image, current: Image.Image) -> dict[str, object]:
    """Classify a current image relative to a fixed reference.

    The result is JSON-friendly and always contains ``kind``, ``pixel_equal``,
    ``blur_sigma``, ``contrast_gain``, ``correlation`` and normalized RMS
    ``residual``. ``pixel_equal`` compares normalized RGBA pixels, while image
    similarity and blur fitting use luminance. A Gaussian radius is evidence
    about the current image only when ``kind`` is ``blur_candidate``;
    ``unassessable`` results can include a reverse-fit ``spatial_direction``
    when the current image is sharper than the reference.
    """
    if not isinstance(reference, Image.Image) or not isinstance(current, Image.Image):
        return _result("unassessable", reason="inputs_must_be_pillow_images")

    if reference.size != current.size:
        return _result(
            "geometry_change",
            blur_sigma=None,
            contrast_gain=None,
            correlation=None,
            residual=None,
            reference_size=[int(reference.width), int(reference.height)],
            current_size=[int(current.width), int(current.height)],
        )

    pixel_equal = _same_color_pixels(reference, current)
    if reference.width < 2 or reference.height < 2:
        return _result(
            "unassessable",
            pixel_equal=pixel_equal,
            blur_sigma=None,
            contrast_gain=None,
            correlation=None,
            residual=None,
            reason="image_too_small",
        )

    reference_pixels = _gray_pixels(reference)
    current_pixels = _gray_pixels(current)
    reference_texture = (
        float(np.std(reference_pixels)) >= _MIN_TEXTURE_STDDEV
        and _edge_rms(reference_pixels) >= _MIN_EDGE_RMS
    )
    current_texture = (
        float(np.std(current_pixels)) >= _MIN_TEXTURE_STDDEV
        and _edge_rms(current_pixels) >= _MIN_EDGE_RMS
    )
    if not reference_texture or not current_texture:
        return _result(
            "unassessable",
            blur_sigma=None,
            contrast_gain=None,
            correlation=None,
            residual=None,
            pixel_equal=pixel_equal,
            reason="insufficient_edge_texture",
        )

    if pixel_equal:
        return _result(
            "unchanged",
            pixel_equal=True,
            blur_sigma=0.0,
            contrast_gain=1.0,
            correlation=1.0,
            residual=0.0,
        )

    if np.array_equal(reference_pixels, current_pixels):
        return _result(
            "content_change",
            pixel_equal=False,
            blur_sigma=0.0,
            contrast_gain=1.0,
            correlation=1.0,
            residual=0.0,
            reason="chromatic_change_at_equal_luminance",
        )

    affine_fit = _fit_affine(reference_pixels, current_pixels)
    affine_details = {
        "contrast_gain": float(affine_fit["gain"]),
        "correlation": float(affine_fit["correlation"]),
        "residual": float(affine_fit["residual"]),
    }

    if affine_fit["residual"] <= _UNCHANGED_RESIDUAL:
        if (
            abs(affine_fit["gain"] - 1.0) <= 0.025
            and abs(affine_fit["offset"]) <= 2.5
        ):
            return _result(
                "unchanged",
                pixel_equal=False,
                blur_sigma=0.0,
                **affine_details,
                reason="affine_equivalent_within_tolerance",
            )
        return _result(
            "contrast",
            pixel_equal=False,
            blur_sigma=0.0,
            **affine_details,
            reason="affine_contrast_within_tolerance",
        )

    forward_fit = _best_gaussian_fit(reference.convert("L"), current_pixels)
    current_signal_rms = float(np.std(current_pixels) / 255.0)
    reference_signal_rms = float(np.std(reference_pixels) / 255.0)
    forward_candidate = _is_blur_evidence(
        affine_fit, forward_fit, current_signal_rms
    )

    reverse_affine = _fit_affine(current_pixels, reference_pixels)
    reverse_fit = _best_gaussian_fit(current.convert("L"), reference_pixels)
    reverse_candidate = _is_blur_evidence(
        reverse_affine, reverse_fit, reference_signal_rms
    )
    forward_improvement = _spatial_improvement(affine_fit, forward_fit)[0]
    reverse_improvement = _spatial_improvement(reverse_affine, reverse_fit)[0]

    if reverse_candidate and (
        not forward_candidate or reverse_improvement > forward_improvement
    ):
        return _result(
            "unassessable",
            pixel_equal=False,
            blur_sigma=0.0,
            contrast_gain=float(reverse_fit["gain"]),
            correlation=float(reverse_fit["correlation"]),
            residual=float(reverse_fit["residual"]),
            spatial_direction="sharper",
            reverse_sigma=float(reverse_fit["sigma"]),
            reason="current_is_sharper_than_reference",
        )

    if forward_candidate:
        absolute, relative = _spatial_improvement(affine_fit, forward_fit)
        return _result(
            "blur_candidate",
            pixel_equal=False,
            blur_sigma=float(forward_fit["sigma"]),
            contrast_gain=float(forward_fit["gain"]),
            correlation=float(forward_fit["correlation"]),
            residual=float(forward_fit["residual"]),
            affine_residual=float(affine_fit["residual"]),
            spatial_improvement=float(absolute),
            relative_improvement=float(relative),
        )

    if affine_fit["correlation"] < 0.95 or affine_fit["residual"] > 0.025:
        return _result(
            "content_change",
            pixel_equal=False,
            blur_sigma=0.0,
            **affine_details,
        )

    return _result(
        "unassessable",
        pixel_equal=False,
        blur_sigma=0.0,
        **affine_details,
        reason="no_conservative_spatial_match",
    )
