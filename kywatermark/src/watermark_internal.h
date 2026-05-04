/*
 * Project Kyber: watermark_internal.h
 * Copyright © 2022-2026 Kyber SAS
 * SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
 *
 * Internal context shape for kywatermark. Not installed.
 */

#ifndef KYWATERMARK_WATERMARK_INTERNAL_H
#define KYWATERMARK_WATERMARK_INTERNAL_H

#include "kywatermark/watermark.h"

#include <libavutil/buffer.h>
#include <libavutil/frame.h>
#include <libavutil/pixfmt.h>

#include <libplacebo/gpu.h>
#include <libplacebo/log.h>
#include <libplacebo/renderer.h>

#if defined(_WIN32)
#include <libplacebo/d3d11.h>
#endif

#if defined(__linux__)
#include <libplacebo/vulkan.h>
#endif

#include <stdbool.h>

#define KYWATERMARK_MAX_WATERMARKS 16
#define KYWATERMARK_LOG_TAG "kywatermark"

/* One placebo-uploaded watermark: stores the decoded RGBA AVFrame, the GPU
 * textures it was mapped to, and the pl_frame metadata libplacebo uses
 * when compositing. The user-supplied geometry/opacity is kept verbatim. */
typedef struct kywatermark {
    /* User parameters */
    float x, y;          /* anchor in [-1, 1] */
    float opacity;       /* TODO: not yet wired into render_params */
    uint32_t img_width;
    uint32_t img_height;

    /* Decoded RGBA image, kept alive for the GPU mapping. */
    AVFrame *avframe;

    /* Mapped placebo frame + backing textures. Owned by us; released in
     * kywatermark_free via pl_unmap_avframe + av_free + av_frame_free. */
    struct pl_frame *plframe;
    pl_tex tex[4];
} kywatermark;

struct kywatermark_ctx {
    /* Caller-provided hwframe device (vaapi on Linux, d3d11va on Windows).
     * We hold our own ref. */
    AVBufferRef *hw_device;

    /* libplacebo state */
    pl_log log;

#if defined(_WIN32)
    pl_d3d11 d3d11;
#endif
#if defined(__linux__)
    pl_vulkan vk;
#endif

    /* Resolved at alloc time from whichever backend got created. */
    pl_gpu gpu;
    pl_renderer renderer;

    /* Layers, in z-order (watermark[0] is bottom). */
    kywatermark watermarks[KYWATERMARK_MAX_WATERMARKS];
    unsigned nb_watermarks;

    /* Detected on the first apply() call from the input frame's sw_format.
     * libplacebo blends by channel name, but BGRA video formats hand
     * libplacebo a frame whose plane[0] components are (B, G, R, A). The
     * watermark RGBA upload has (R, G, B, A). Without remapping, the blend
     * sends R-of-watermark into B-of-video. */
    bool is_bgr;
    bool sw_format_known;

    /* Diagnostic: counts apply() invocations, used as a low-rate health
     * check and for the Linux no-op trace. */
    unsigned long frames_processed;
};

#endif /* KYWATERMARK_WATERMARK_INTERNAL_H */
