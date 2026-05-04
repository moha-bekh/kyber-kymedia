/* Raw FFI wrapper for kywatermark.
 *
 * Pulls in libavutil for AVBufferRef / AVFrame, then the kywatermark public
 * header. libplacebo headers are NOT included here - kywatermark's own
 * header keeps the placebo types out of the public API. */

#include <libavutil/avutil.h>
#include <libavutil/buffer.h>
#include <libavutil/frame.h>

#include <kywatermark/watermark.h>
