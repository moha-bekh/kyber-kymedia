/*
 * Project Kyber: watermark.c
 * Copyright © 2022-2026 Kyber SAS
 * SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
 *
 * kywatermark - public API impl: device init, image decode, watermark math,
 * libplacebo render path.
 *
 * Backends:
 *   - Windows: d3d11 via pl_d3d11_create. Frames coming through the FG
 *     hook are AV_PIX_FMT_D3D11 hwframes; the device is extracted from
 *     the hwframe context and handed to libplacebo.
 *   - Linux:   not yet implemented. txproto's path goes through
 *     CUDA->Vulkan derivation which only works on NVIDIA. Until we have
 *     a vendor-agnostic VAAPI->Vulkan import path, the Linux build logs
 *     a warning at alloc time and apply() falls back to a no-op so the
 *     pipeline still runs (just without watermarking).
 */

#define COBJMACROS

#include "kywatermark/watermark.h"
#include "watermark_internal.h"

#include <libavcodec/avcodec.h>
#include <libavcodec/packet.h>
#include <libavutil/buffer.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/log.h>
#include <libavutil/mem.h>
#include <libavutil/pixdesc.h>
#include <libavutil/time.h>
#include <libswscale/swscale.h>

#include <libplacebo/gpu.h>
#include <libplacebo/log.h>
#include <libplacebo/renderer.h>
#include <libplacebo/utils/libav.h>

#if defined(_WIN32)
#include <libavutil/hwcontext_d3d11va.h>
#include <libplacebo/d3d11.h>
#endif

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void pl_log_cb(void *priv, enum pl_log_level level, const char *msg)
{
    (void)priv;
    int av_level;
    switch (level) {
    case PL_LOG_FATAL: av_level = AV_LOG_FATAL;   break;
    case PL_LOG_ERR:   av_level = AV_LOG_ERROR;   break;
    case PL_LOG_WARN:  av_level = AV_LOG_WARNING; break;
    case PL_LOG_INFO:  av_level = AV_LOG_INFO;    break;
    case PL_LOG_DEBUG: av_level = AV_LOG_DEBUG;   break;
    case PL_LOG_TRACE: av_level = AV_LOG_TRACE;   break;
    default:           return;
    }
    av_log(NULL, av_level, "[" KYWATERMARK_LOG_TAG "/placebo] %s\n", msg);
}

static AVFrame *kywatermark_load_image(const char *path, enum AVPixelFormat format)
{
    AVFrame *out_frame = NULL;
    AVCodecContext *codec_ctx = NULL;
    AVPacket *pkt = NULL;
    AVFrame *frame = NULL;
    uint8_t *file_bytes = NULL;
    struct SwsContext *sws = NULL;
    FILE *f = NULL;
    int ret;

    f = fopen(path, "rb");
    if (!f) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] could not open %s\n", path);
        return NULL;
    }

    av_log(NULL, AV_LOG_INFO, "[" KYWATERMARK_LOG_TAG "] %s opened\n", path);

    const AVCodec *codec = avcodec_find_decoder(AV_CODEC_ID_PNG);
    if (!codec) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] AV_CODEC_ID_PNG not supported\n");
        goto end;
    }

    codec_ctx = avcodec_alloc_context3(codec);
    pkt = av_packet_alloc();
    frame = av_frame_alloc();
    if (!codec_ctx || !pkt || !frame) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] allocation failure\n");
        goto end;
    }

    if ((ret = avcodec_open2(codec_ctx, codec, NULL)) < 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] could not open PNG codec: %s\n",
               av_err2str(ret));
        goto end;
    }

    if (fseek(f, 0, SEEK_END) != 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] seek failed\n");
        goto end;
    }
    long fsize_long = ftell(f);
    if (fsize_long <= 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] empty or unreadable file\n");
        goto end;
    }
    rewind(f);

    size_t fsize = (size_t)fsize_long;
    file_bytes = av_malloc(fsize);
    if (!file_bytes) {
        av_log(NULL, AV_LOG_ERROR, "[" KYWATERMARK_LOG_TAG "] OOM\n");
        goto end;
    }

    size_t nread = fread(file_bytes, 1, fsize, f);
    if (nread != fsize) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] short read (%zu/%zu)\n", nread, fsize);
        goto end;
    }

    ret = av_new_packet(pkt, (int)fsize);
    if (ret < 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] av_new_packet failed: %s\n",
               av_err2str(ret));
        goto end;
    }
    memcpy(pkt->data, file_bytes, fsize);

    if ((ret = avcodec_send_packet(codec_ctx, pkt)) < 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] error sending packet: %s\n",
               av_err2str(ret));
        goto end;
    }

    ret = avcodec_receive_frame(codec_ctx, frame);
    if (ret < 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] error decoding: %s\n",
               av_err2str(ret));
        goto end;
    }

    sws = sws_getContext(
        frame->width, frame->height, (enum AVPixelFormat)frame->format,
        frame->width, frame->height, format,
        SWS_BILINEAR, NULL, NULL, NULL);
    if (!sws) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] sws_getContext failed\n");
        goto end;
    }

    out_frame = av_frame_alloc();
    if (!out_frame) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] OOM for output frame\n");
        goto end;
    }

    out_frame->width = frame->width;
    out_frame->height = frame->height;
    out_frame->format = format;

    ret = av_frame_get_buffer(out_frame, 0);
    if (ret < 0) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] av_frame_get_buffer failed: %s\n",
               av_err2str(ret));
        av_frame_free(&out_frame);
        out_frame = NULL;
        goto end;
    }

    int h = sws_scale(sws,
                      (const uint8_t * const *)frame->data, frame->linesize,
                      0, frame->height,
                      out_frame->data, out_frame->linesize);
    if (h != frame->height) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] sws_scale failed (%d/%d)\n",
               h, frame->height);
        av_frame_free(&out_frame);
        out_frame = NULL;
        goto end;
    }

end:
    if (sws)        sws_freeContext(sws);
    if (file_bytes) av_free(file_bytes);
    if (frame)      av_frame_free(&frame);
    if (pkt)        av_packet_free(&pkt);
    if (codec_ctx)  avcodec_free_context(&codec_ctx);
    if (f)          fclose(f);
    return out_frame;
}

static void update_watermark_crop(const kywatermark *watermark,
                              const AVFrame *frame,
                              struct pl_frame *target_frame)
{
    if (!frame || !target_frame)
        return;

    const float frame_width  = (float)frame->width;
    const float frame_height = (float)frame->height;

    float target_width  = watermark->img_width  > 0 ? (float)watermark->img_width  : 1.0f;
    float target_height = watermark->img_height > 0 ? (float)watermark->img_height : 1.0f;

    if (frame_width > 0.0f && target_width > frame_width) {
        const float scale = frame_width / target_width;
        target_width  = frame_width;
        target_height *= scale;
    }
    if (frame_height > 0.0f && target_height > frame_height) {
        const float scale = frame_height / target_height;
        target_height = frame_height;
        target_width  *= scale;
    }

    target_width  = fmaxf(target_width,  1.0f);
    target_height = fmaxf(target_height, 1.0f);

    float pos_x = fminf(fmaxf(watermark->x, -1.0f), 1.0f);
    float pos_y = fminf(fmaxf(watermark->y, -1.0f), 1.0f);

    /* Map [-1, 1] -> [0, 1] */
    float norm_x = (pos_x + 1.0f) / 2.0f;
    float norm_y = (1.0f - pos_y) / 2.0f;

    float max_x = fmaxf(frame_width  - target_width,  0.0f);
    float max_y = fmaxf(frame_height - target_height, 0.0f);

    float crop_x0 = fminf(fmaxf(max_x * norm_x, 0.0f), max_x);
    float crop_y0 = fminf(fmaxf(max_y * norm_y, 0.0f), max_y);

    target_frame->crop.x0 = crop_x0;
    target_frame->crop.y0 = crop_y0;
    target_frame->crop.x1 = crop_x0 + target_width;
    target_frame->crop.y1 = crop_y0 + target_height;
}

#if defined(_WIN32)
static int backend_init_d3d11(struct kywatermark_ctx *ctx)
{
    AVHWDeviceContext *dev_ctx = (AVHWDeviceContext *)ctx->hw_device->data;
    if (dev_ctx->type != AV_HWDEVICE_TYPE_D3D11VA) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] expected D3D11VA hw_device, got type %d\n",
               (int)dev_ctx->type);
        return AVERROR(EINVAL);
    }

    AVD3D11VADeviceContext *d3d = (AVD3D11VADeviceContext *)dev_ctx->hwctx;

    struct pl_d3d11_params params = { .device = d3d->device };
    ctx->d3d11 = pl_d3d11_create(ctx->log, &params);
    if (!ctx->d3d11) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] pl_d3d11_create failed\n");
        return AVERROR_EXTERNAL;
    }

    ctx->gpu = ctx->d3d11->gpu;
    return 0;
}

static void backend_destroy_d3d11(struct kywatermark_ctx *ctx)
{
    if (ctx->d3d11)
        pl_d3d11_destroy(&ctx->d3d11);
}
#endif /* _WIN32 */

static void kywatermark_prewarm(struct kywatermark_ctx *ctx, kywatermark *watermark)
{
    if (!ctx->gpu || !ctx->renderer || !watermark->plframe)
        return;

    pl_fmt fmt = pl_find_named_fmt(ctx->gpu, "rgba8");
    if (!fmt) {
        av_log(NULL, AV_LOG_WARNING,
               "[" KYWATERMARK_LOG_TAG "] prewarm: rgba8 format not found, skipping\n");
        return;
    }

    pl_tex tex = pl_tex_create(ctx->gpu, pl_tex_params(
        .w          = 16,
        .h          = 16,
        .format     = fmt,
        .renderable = true,
    ));
    if (!tex) {
        av_log(NULL, AV_LOG_WARNING,
               "[" KYWATERMARK_LOG_TAG "] prewarm: pl_tex_create failed, skipping\n");
        return;
    }

    struct pl_frame target = {
        .num_planes = 1,
        .planes = {{
            .texture           = tex,
            .components        = 4,
            .component_mapping = { 0, 1, 2, 3 },
        }},
        .crop = { 0, 0, 16, 16 },
    };
    target.repr.sys    = PL_COLOR_SYSTEM_RGB;
    target.repr.levels = PL_COLOR_LEVELS_FULL;

    /* Stretch the watermark to fill the 16x16 dummy target. */
    watermark->plframe->crop.x0 = 0.0;
    watermark->plframe->crop.y0 = 0.0;
    watermark->plframe->crop.x1 = (float)watermark->img_width;
    watermark->plframe->crop.y1 = (float)watermark->img_height;

    struct pl_render_params params = pl_render_default_params;
    params.border       = PL_CLEAR_SKIP;
    params.background   = PL_CLEAR_SKIP;
    params.blend_params = &pl_alpha_overlay;

    int64_t t0 = av_gettime_relative();
    bool ok = pl_render_image(ctx->renderer, watermark->plframe, &target, &params);
    int64_t dt_us = av_gettime_relative() - t0;

    pl_tex_destroy(ctx->gpu, &tex);

    if (!ok) {
        av_log(NULL, AV_LOG_WARNING,
               "[" KYWATERMARK_LOG_TAG "] prewarm: pl_render_image failed (continuing)\n");
        return;
    }

    av_log(NULL, AV_LOG_INFO,
           "[" KYWATERMARK_LOG_TAG "] prewarm: shaders pre-compiled in %.1f ms\n",
           dt_us / 1000.0);
}

kywatermark_ctx *kywatermark_alloc(AVBufferRef *hw_device)
{
    if (!hw_device) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] alloc: hw_device is NULL\n");
        return NULL;
    }

    kywatermark_ctx *ctx = av_mallocz(sizeof(*ctx));
    if (!ctx)
        return NULL;

    ctx->hw_device = av_buffer_ref(hw_device);
    if (!ctx->hw_device) {
        av_free(ctx);
        return NULL;
    }

    ctx->log = pl_log_create(PL_API_VER, pl_log_params(
        .log_level = PL_LOG_INFO,
        .log_cb    = pl_log_cb,
    ));
    if (!ctx->log) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] pl_log_create failed\n");
        goto fail;
    }

#if defined(_WIN32)
    if (backend_init_d3d11(ctx) < 0)
        goto fail;
#elif defined(__linux__)
    /* TODO: VAAPI -> Vulkan import path. txproto goes via CUDA which is
     * NVIDIA-only; we want a vendor-agnostic path. Until then, alloc
     * succeeds but apply() is a no-op so the pipeline keeps running. */
    av_log(NULL, AV_LOG_WARNING,
           "[" KYWATERMARK_LOG_TAG "] alloc: Linux backend not yet implemented; "
           "watermark will be a no-op\n");
    ctx->gpu = NULL;
    return ctx;
#else
    av_log(NULL, AV_LOG_ERROR,
           "[" KYWATERMARK_LOG_TAG "] alloc: no backend for this platform\n");
    goto fail;
#endif

    ctx->renderer = pl_renderer_create(ctx->log, ctx->gpu);
    if (!ctx->renderer) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] pl_renderer_create failed\n");
        goto fail;
    }

    av_log(NULL, AV_LOG_INFO,
           "[" KYWATERMARK_LOG_TAG "] alloc: context ready\n");
    return ctx;

fail:
    kywatermark_free(&ctx);
    return NULL;
}

int kywatermark_add(kywatermark_ctx *ctx, const kywatermark_params *params)
{
    if (!ctx || !params || !params->image_path)
        return AVERROR(EINVAL);

    if (ctx->nb_watermarks >= KYWATERMARK_MAX_WATERMARKS) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] add: limit (%d) reached\n",
               KYWATERMARK_MAX_WATERMARKS);
        return AVERROR(ENOMEM);
    }

    kywatermark *watermark = &ctx->watermarks[ctx->nb_watermarks];
    memset(watermark, 0, sizeof(*watermark));
    watermark->x       = params->x;
    watermark->y       = params->y;
    watermark->opacity = params->opacity;

    /* Decode PNG to RGBA */
    watermark->avframe = kywatermark_load_image(params->image_path, AV_PIX_FMT_RGBA);
    if (!watermark->avframe) {
        av_log(NULL, AV_LOG_ERROR,
               "[" KYWATERMARK_LOG_TAG "] add: failed to load %s\n",
               params->image_path);
        return AVERROR_EXTERNAL;
    }
    watermark->img_width  = watermark->avframe->width;
    watermark->img_height = watermark->avframe->height;

    /* Map to GPU. Skipped on the no-op Linux path where we don't have a
     * gpu - the watermark is registered but apply() short-circuits. */
    if (ctx->gpu) {
        watermark->plframe = av_mallocz(sizeof(struct pl_frame));
        if (!watermark->plframe) {
            av_frame_free(&watermark->avframe);
            return AVERROR(ENOMEM);
        }

        bool ok = pl_map_avframe_ex(ctx->gpu, watermark->plframe,
            pl_avframe_params(
                .frame = watermark->avframe,
                .tex   = watermark->tex,
            ));
        if (!ok) {
            av_log(NULL, AV_LOG_ERROR,
                   "[" KYWATERMARK_LOG_TAG "] add: pl_map_avframe_ex failed\n");
            av_freep(&watermark->plframe);
            av_frame_free(&watermark->avframe);
            return AVERROR_EXTERNAL;
        }

        /* PNG is sRGB / full-range RGB - tell libplacebo so the
         * compositor doesn't try to apply a video color-space conversion
         * to the watermark. */
        watermark->plframe->repr.sys    = PL_COLOR_SYSTEM_RGB;
        watermark->plframe->repr.levels = PL_COLOR_LEVELS_FULL;

        /* Component remapping for BGR video targets is applied lazily on
         * the first apply() call once we know the input sw_format. */

        /* Force shader compilation now (setup-time, not on the hot path). */
        kywatermark_prewarm(ctx, watermark);
    }

    ctx->nb_watermarks++;
    av_log(NULL, AV_LOG_INFO,
           "[" KYWATERMARK_LOG_TAG "] add: %s @ (%.2f, %.2f), opacity %.2f, %ux%u (watermark %u)\n",
           params->image_path, params->x, params->y, params->opacity,
           watermark->img_width, watermark->img_height, ctx->nb_watermarks);
    return 0;
}

static void detect_and_fix_bgr(struct kywatermark_ctx *ctx, const AVFrame *frame)
{
    if (ctx->sw_format_known)
        return;

    enum AVPixelFormat sw_fmt = (enum AVPixelFormat)frame->format;
    if (frame->hw_frames_ctx) {
        AVHWFramesContext *fc = (AVHWFramesContext *)frame->hw_frames_ctx->data;
        sw_fmt = fc->sw_format;
    }

    const char *fmt_name = av_get_pix_fmt_name(sw_fmt);
    ctx->is_bgr = fmt_name && (strstr(fmt_name, "bgr") || strstr(fmt_name, "BGR"));
    ctx->sw_format_known = true;

    av_log(NULL, AV_LOG_INFO,
           "[" KYWATERMARK_LOG_TAG "] input sw_format=%s, is_bgr=%d\n",
           fmt_name ? fmt_name : "?", (int)ctx->is_bgr);
}

int kywatermark_apply(kywatermark_ctx *ctx, AVFrame *frame)
{
    if (!ctx || !frame)
        return AVERROR(EINVAL);

    ctx->frames_processed++;

    /* Linux no-op fallback path (and any case where alloc didn't bind a
     * GPU). Periodic log for diagnostics. */
    if (!ctx->gpu || !ctx->renderer || ctx->nb_watermarks == 0) {
        if (ctx->frames_processed == 1 || ctx->frames_processed % 300 == 0) {
            av_log(NULL, AV_LOG_INFO,
                   "[" KYWATERMARK_LOG_TAG "] apply: %lu frames processed (no-op)\n",
                   ctx->frames_processed);
        }
        return 0;
    }

    detect_and_fix_bgr(ctx, frame);

    /* Map the input frame as the render target. pl_map_avframe_ex with a
     * hwframe (d3d11 here) wires placebo straight to the underlying GPU
     * textures; no CPU bounce. */
    struct pl_frame in_frame;
    pl_tex in_tex[4];

    bool ok = pl_map_avframe_ex(ctx->gpu, &in_frame,
        pl_avframe_params(
            .frame = frame,
            .tex   = in_tex,
        ));
    if (!ok) {
        av_log(NULL, AV_LOG_WARNING,
               "[" KYWATERMARK_LOG_TAG "] apply: pl_map_avframe_ex(input) failed\n");
        return AVERROR_EXTERNAL;
    }

    struct pl_render_params render_params = pl_render_default_params;
    render_params.border       = PL_CLEAR_SKIP;
    render_params.background   = PL_CLEAR_SKIP;
    render_params.blend_params = &pl_alpha_overlay;

    int ret = 0;
    for (unsigned i = 0; i < ctx->nb_watermarks; i++) {
        kywatermark *watermark = &ctx->watermarks[i];
        if (!watermark->plframe)
            continue;

        /* Each watermark carries its own crop computed from this frame's
         * dimensions. The input frame's color space is copied onto the
         * watermark so the compositor doesn't try to convert the RGBA
         * watermark into the video's native color system. */
        watermark->plframe->color = in_frame.color;
        update_watermark_crop(watermark, frame, &in_frame);

        ok = pl_render_image(ctx->renderer,
                             (const struct pl_frame *)watermark->plframe,
                             &in_frame,
                             &render_params);
        if (!ok) {
            av_log(NULL, AV_LOG_ERROR,
                   "[" KYWATERMARK_LOG_TAG "] apply: pl_render_image failed (watermark %u)\n", i);
            ret = AVERROR_EXTERNAL;
            /* Continue with remaining watermarks - partial watermark is better
             * than a stalled stream. */
        }
    }

    pl_unmap_avframe(ctx->gpu, &in_frame);

    if (ctx->frames_processed == 1 || ctx->frames_processed % 600 == 0) {
        av_log(NULL, AV_LOG_INFO,
               "[" KYWATERMARK_LOG_TAG "] apply: %lu frames composited\n",
               ctx->frames_processed);
    }

    return ret;
}

void kywatermark_free(kywatermark_ctx **pctx)
{
    if (!pctx || !*pctx)
        return;

    kywatermark_ctx *ctx = *pctx;

    /* Drop watermarks first (they reference textures owned by the renderer/gpu). */
    for (unsigned i = 0; i < ctx->nb_watermarks; i++) {
        kywatermark *watermark = &ctx->watermarks[i];
        if (watermark->plframe && ctx->gpu) {
            pl_unmap_avframe(ctx->gpu, watermark->plframe);
        }
        av_freep(&watermark->plframe);
        av_frame_free(&watermark->avframe);
    }
    ctx->nb_watermarks = 0;

    if (ctx->renderer)
        pl_renderer_destroy(&ctx->renderer);

#if defined(_WIN32)
    backend_destroy_d3d11(ctx);
#endif

    if (ctx->log)
        pl_log_destroy(&ctx->log);

    av_buffer_unref(&ctx->hw_device);
    av_free(ctx);
    *pctx = NULL;
}
