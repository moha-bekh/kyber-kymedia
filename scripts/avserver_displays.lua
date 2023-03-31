video_display_list = {}
video_display_list_json = nil

function enumerate_display(identifier, entry)
    if entry.type ~= "display" then
        return
    end

    local entry = {
        id = entry.api_id,
        name = entry.name,
        width = entry.video.width,
        height = entry.video.height,
        x = entry.video.x,
        y = entry.video.y,
    }

    local updated = false

    for i = 1, #video_display_list do
        if video_display_list[i].id == entry.id then
            video_display_list[i] = entry
            updated = true
            break
        end
    end

    if updated == false then
        table.insert(video_display_list, entry)
    end

    video_display_list_json = format_video_display_list()
    notify_displays()
end

function notify_displays()
    if display_update_cb ~= nil then
        display_update_cb(format_video_display_list())
    end
end

function format_video_display_list()
    local out = "["

    for i = 1, #video_display_list do
        local entry = video_display_list[i]

        out = out .. "{"

        out = out .. "\"id\":" .. entry.id .. ","
        out = out .. "\"name\": \"" .. entry.name:gsub("\\", "\\\\") .. "\","
        out = out .. "\"width\":" .. entry.width .. ","
        out = out .. "\"height\":" .. entry.height .. ","
        out = out .. "\"x\":" .. entry.x .. ","
        out = out .. "\"y\":" .. entry.y

        if i < #video_display_list then
            out = out .. "},"
        else
            out = out .. "}"
        end
    end

    out = out .. "]"

    return out
end

function main(...)
    tx.register_io_cb(enumerate_display)
end
