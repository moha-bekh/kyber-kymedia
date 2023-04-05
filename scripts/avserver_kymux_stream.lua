requested_display_api_id = nil

video_display_id = nil
audio_monitor_id = nil

function io_update_cb(identifier, entry)
    tx.log("Add entry", entry.name, entry.api_id)

    if video_display_id == nil and entry.type == "display" and entry.api_id == requested_display_api_id then
        tx.log("Use video display", identifier, entry.name)
        video_display_id = identifier
    end

    if audio_monitor_id == nil and entry.type == "monitor" and entry.default then
        tx.log("Use audio monitor", entry.name)
        audio_monitor_id = identifier
    end
end

function muxer_stats(stats)
    statusline = "Encoding, bitrate: " .. math.floor(stats.bitrate / 1000) .. " Kbps"
    tx.set_status(statusline)
end

function create_x264(source_video, codec, yuv444, bitrate)
    local filter = tx.create_filtergraph({
        graph = "scale=w=1920:h=-1,format=nv12",
        priv_options = { dump_graph = false },
    })

    local encoder = tx.create_encoder({
        encoder = "libx264",
        options = {
            preset = "ultrafast",
            tune = "zerolatency",
            flags = "+global_header",
            b = bitrate,
        }
    })

    filter.link(source_video)
    encoder.link(filter)

    return filter, encoder
end

function create_nvenc(source_video, codec, yuv444, bitrate)
    local options = {
        preset = "p2",
        tune = "ull",
        zerolatency = true,
        delay = 1,
        bf = 0,
        b_ref_mode = 0,
        b = bitrate,
        flags = "+global_header",
    }

    if codec == "h264" then
        if yuv444 == "true" then
            options.profile = "high444p"
        else
            options.profile = "high"
        end
    end

    local encoder = tx.create_encoder({
        encoder = string.format("%s_nvenc", codec),
        priv_options = { fifo_size = 1 },
        options = options,
    })

    encoder.link(source_video)

    return nil, encoder
end

function create_qsv(source_video, codec, yuv444, bitrate)
    local filter = tx.create_filtergraph({
        graph = "hwmap,format=qsv,scale_qsv=format=nv12",
        hwctx = "qsv",
        priv_options = { dump_graph = false },
    })

    local options = {
        preset = "veryfast",
        bf = 0,
        g = 600,
        b = bitrate,
        async_depth = 1,
        flags = "+global_header",
    }

    if codec == "h264" then
        options.profile = "high"
    end

    local encoder = tx.create_encoder({
        encoder = string.format("%s_qsv", codec),
        priv_options = { fifo_size = 1 },
        options = options,
    })

    filter.link(source_video)
    encoder.link(filter)

    return filter, encoder
end

function create_amf(source_video, codec, yuv444, bitrate)
    local filter = tx.create_filtergraph({
        graph = "hwdownload,format=bgra,hwupload",
        hwctx = "d3d11va",
        priv_options = { dump_graph = true },
    })

    local options = {
        b = bitrate,
        bf = 0,
        flags = "+global_header",
    }

    if codec == "h264" then
        options.usage = "ultralowlatency"
        options.profile = "high"
    elseif codec == "hevc" then
        options.usage = "ultralowlatency"
    elseif codec == "av1" then
        options.usage = "lowlatency"
    end

    local encoder = tx.create_encoder({
        encoder = string.format("%s_amf", codec),
        priv_options = { fifo_size = 1 },
        options = options
    })

    filter.link(source_video)
    encoder.link(filter)

    return filter, encoder
end

function create_vaapi(source_video, codec, yuv444, bitrate)
    local options = {
        b = bitrate,
        bf = 0,
        flags = "+global_header",
    }

    if codec == "h264" then
        options.profile = "high"
    end

    local encoder = tx.create_encoder({
        encoder = string.format("%s_vaapi", codec),
        pix_fmt = "nv12",
        priv_options = { fifo_size = 2 },
        options = options,
    })

    encoder.link(source_video)

    return nil, encoder
end

function main_video(...)
    local arg = {...}
    local display_id, uri, encoder_name, codec, yuv444 = arg[1], arg[2], arg[3], arg[4], arg[5], arg[6]
    local bitrate = "20M"

    requested_display_api_id = tonumber(display_id)

    event = tx.register_io_cb(io_update_cb)
    event.destroy()

    tx.set_epoch(0)

    --[[ VIDEO ]] --
    source_video = tx.create_io(video_display_id, {
        width = 1920,
        height = 1080,
    })

    local encoder_map = {
        x264 = create_x264,
        nvenc = create_nvenc,
        qsv = create_qsv,
        amf = create_amf,
        vaapi = create_vaapi,
    }

    local encoder_create_cb = encoder_map[encoder_name]
    if encoder_create_cb == nil then
        tx.log(string.format("Encoder %s unknown", encoder_name))
        -- Continue to let the lua interpreter quit. os.exit() isn't
        -- enabled.
    end

    local filter_v, encoder_v = encoder_create_cb(source_video, codec, yuv444, bitrate)

    packet_sink = tx.create_packet_sink({
        uri = uri,
        stream_id = 0, -- hardcoded: must be 0 for video
    })
    packet_sink.link(encoder_v)

    tx.commit()
end

function main_audio(...)
    local arg = {...}
    uri = arg[1]

    event = tx.register_io_cb(io_update_cb)
    event.destroy()

    tx.set_epoch(0)

    source_audio = tx.create_io(audio_monitor_id, {
        buffer_ms = 20,
    })

    encoder = tx.create_encoder({
        encoder = "libopus",
        options = {
            b = 10^3 * 128,
            application = "audio",
            frame_duration = 20,
            vbr = "on",
            flags = "+global_header",
        },
    })
    encoder.link(source_audio)

    packet_sink = tx.create_packet_sink({
        uri = uri,
        stream_id = 1, -- hardcoded: must be 1 for audio
    })
    packet_sink.link(encoder)

    tx.commit()
end
