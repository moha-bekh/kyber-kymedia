// Project Kyber: log_shim.c
// Copyright © 2022-2026 Kyber SAS
// SPDX-License-Identifier: LicenseRef-Kyber-Commercial OR AGPL-3.0-or-later
//
// This file is both under dual license: AGPLv3 and a Commercial one.
//
// ----
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#include <libavutil/log.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stddef.h>

/* Rust sink: receives the level + an already-formatted line and its length
 * (in bytes, no trailing newline), so Rust needs no second strlen pass. */
typedef void (*kyav_log_sink_fn)(int level, const char *line, size_t len);

/* Atomic so installing the sink races cleanly with the log callback, which
 * FFmpeg may invoke concurrently from worker threads. */
static _Atomic(kyav_log_sink_fn) g_sink = NULL;

static void kyav_log_callback(void *avcl, int level, const char *fmt, va_list vl)
{
    kyav_log_sink_fn sink = atomic_load_explicit(&g_sink, memory_order_acquire);
    if (!sink)
        return;
    if (level > av_log_get_level())   /* honour av_log_set_level() */
        return;

    char line[1024];
    int print_prefix = 1;
    int n = av_log_format_line2(avcl, level, fmt, vl,
                                line, sizeof(line), &print_prefix);
    if (n <= 0)
        return;

    /* drop the trailing newline FFmpeg appends (clamp if truncated) */
    int len = n < (int)sizeof(line) ? n : (int)sizeof(line) - 1;
    while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r'))
        len--;

    sink(level, line, (size_t)len);
}

void kyav_install_log_sink(void (*sink)(int, const char *, size_t))
{
    atomic_store_explicit(&g_sink, sink, memory_order_release);
    av_log_set_callback(sink ? kyav_log_callback : av_log_default_callback);
}
