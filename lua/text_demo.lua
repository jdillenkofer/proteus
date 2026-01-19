-- Text Rendering Demo for Proteus LuaCanvas
-- Demonstrates text rendering capabilities with animations

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        t = 0,
        w = 1920,
        h = 1080,
        fonts = nil,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    -- Get available fonts (cached)
    self.fonts = canvas.list_fonts()
    print("Available fonts: " .. #self.fonts)
    for i = 1, math.min(5, #self.fonts) do
        print("  " .. self.fonts[i])
    end
end

function M:update(dt)
    self.t = self.t + dt
end

function M:draw()
    -- Clear to dark gradient-like background
    canvas.clear(25, 25, 35, 255)
    
    -- Draw decorative rectangles
    local r = math.floor(40 + 20 * math.sin(self.t * 0.5))
    canvas.fill_rect(0, 0, self.w, 80, r, 30, 50, 255)
    canvas.fill_rect(0, self.h - 60, self.w, 60, 30, r, 50, 255)
    
    -- Title text with animation
    local title = "Proteus Canvas"
    local title_size = 72
    local tw, th = canvas.measure_text(title, title_size)
    local title_x = (self.w - tw) / 2
    local title_y = 100
    
    -- Animated color for title
    local tr = math.floor(200 + 55 * math.sin(self.t * 2))
    local tg = math.floor(200 + 55 * math.sin(self.t * 2.5))
    local tb = math.floor(200 + 55 * math.sin(self.t * 3))
    
    canvas.draw_text(title_x, title_y, title, title_size, tr, tg, tb, 255)
    
    -- Subtitle
    local subtitle = "Text Rendering Demo"
    local subtitle_size = 36
    local sw, sh = canvas.measure_text(subtitle, subtitle_size)
    local subtitle_x = (self.w - sw) / 2
    canvas.draw_text(subtitle_x, title_y + th + 20, subtitle, subtitle_size, 180, 180, 200, 255)
    
    -- Current time display
    local time_str = string.format("Time: %.2fs", self.t)
    local time_size = 48
    local time_w, _ = canvas.measure_text(time_str, time_size)
    canvas.draw_text((self.w - time_w) / 2, 280, time_str, time_size, 100, 200, 255, 255)
    
    -- Scrolling text demo
    local scroll_text = "Cross-platform text rendering with fontdb + ab_glyph • "
    local scroll_size = 32
    local scroll_w, _ = canvas.measure_text(scroll_text, scroll_size)
    local scroll_offset = (self.t * 100) % (scroll_w * 2)
    
    -- Draw scrolling text twice for seamless loop
    canvas.draw_text(-scroll_offset, self.h - 45, scroll_text .. scroll_text, scroll_size, 200, 200, 220, 255)
    
    -- Font size showcase
    local sizes = {16, 24, 32, 48, 64}
    local y_offset = 380
    for i, size in ipairs(sizes) do
        local sample_text = "Size " .. size .. "px - The quick brown fox"
        local alpha = math.floor(255 - (i - 1) * 30)
        canvas.draw_text(100, y_offset, sample_text, size, 220, 220, 240, alpha)
        y_offset = y_offset + size + 20
    end
    
    -- Measure text demo - draw a box around measured text
    local measure_demo = "Measured Text"
    local measure_size = 40
    local mw, mh = canvas.measure_text(measure_demo, measure_size)
    local mx, my = self.w - mw - 100, 380
    
    -- Draw background rectangle behind text
    canvas.fill_rect(mx - 10, my - 5, mw + 20, mh + 10, 60, 60, 80, 200)
    canvas.stroke_rect(mx - 10, my - 5, mw + 20, mh + 10, 100, 150, 255, 255, 2)
    canvas.draw_text(mx, my, measure_demo, measure_size, 255, 255, 255, 255)
end

function M:save_state()
    return { t = self.t, w = self.w, h = self.h }
end

function M:load_state(state)
    self.t = state.t or self.t
    self.w = state.w or self.w
    self.h = state.h or self.h
end

return M
