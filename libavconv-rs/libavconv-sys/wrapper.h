/* Raw FFI wrapper for libavconv and libavgrab */
#include <libavutil/avutil.h>
#include <libavutil/rational.h>
#include <libavutil/buffer.h>
#include <libavutil/hwcontext.h>
#include <libavutil/opt.h>
#include <libavutil/pixfmt.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/refstruct.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>

#include <libavconv/avconv.h>
#include <libavconv/handle.h>
#include <libavconv/dsink.h>
#include <libavconv/filter.h>
#include <libavconv/enc.h>
#include <libavconv/mux.h>
#include <libavconv/params.h>

#include <libavgrab/avgrab.h>
#include <libavgrab/side_data.h>
#include <libavutil/time.h>
