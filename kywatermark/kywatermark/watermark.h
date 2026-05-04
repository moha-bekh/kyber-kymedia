/*
 * Project Kyber: watermark.h
 * Copyright © 2022-2026 Kyber SAS
 * SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
 *
 * Public C API for kywatermark: a small libplacebo-backed library that
 * composites N image watermarks (PNG, RGBA) onto an AVFrame in its native
 * hwframe context (Vulkan / d3d11) - zero CPU roundtrip.
 *
 * Watermarking model: watermarks are added once at setup time via
 * `kywatermark_add()` and rendered in order on every
 * `kywatermark_apply()` call. Position is anchored in [-1, 1] coordinates
 * (-1 = left/top, 0 = center, +1 = right/bottom).
 */

#ifndef KYWATERMARK_WATERMARK_H
#define KYWATERMARK_WATERMARK_H

#include <libavutil/buffer.h>
#include <libavutil/frame.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct kywatermark_ctx kywatermark_ctx;

typedef struct kywatermark_params {
    /* PNG file path on disk. Image is decoded once at add time and
     * uploaded to GPU; the file no longer needs to exist after that. */
    const char *image_path;

    /* Anchor position. (-1, -1) = top-left, (0, 0) = center,
     * (1, 1) = bottom-right. Matches kyavservice's WatermarkConfig. */
    float x;
    float y;

    /* Watermark opacity in [0.0, 1.0]. 1.0 = fully opaque. */
    float opacity;
} kywatermark_params;

/**
 * Allocate an watermark context bound to a libavutil hwframe device context.
 *
 * `hw_device` must be an AVBufferRef wrapping an AVHWDeviceContext (vaapi
 * on Linux, d3d11va on Windows). The context takes its own reference;
 * the caller retains theirs.
 *
 * Returns NULL on allocation failure.
 */
kywatermark_ctx *kywatermark_alloc(AVBufferRef *hw_device);

/**
 * Add a watermark. Loads the PNG, uploads to GPU, retains it for the lifetime
 * of the context.
 *
 * Returns 0 on success, AVERROR(*) on failure (file not found, decode
 * error, GPU upload failure).
 */
int kywatermark_add(kywatermark_ctx *ctx,
                        const kywatermark_params *params);

/**
 * Composite all added watermarks onto `frame` in place. Layers are drawn in
 * the order they were added (last added on top).
 *
 * `frame` must be a hwframe in the same hwframe context as `hw_device`
 * passed to `kywatermark_alloc`.
 *
 * This is a no-op if no watermarks have been added.
 *
 * Returns 0 on success, AVERROR(*) on failure.
 */
int kywatermark_apply(kywatermark_ctx *ctx, AVFrame *frame);

/**
 * Free the context, all watermarks, and GPU resources. Sets `*ctx` to NULL.
 * Safe to call with `*ctx == NULL`.
 */
void kywatermark_free(kywatermark_ctx **ctx);

#ifdef __cplusplus
}
#endif

#endif /* KYWATERMARK_WATERMARK_H */
