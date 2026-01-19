-- Digital Clock for Proteus LuaCanvas
-- Displays current time with a sleek digital clock design

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        t = 0,
        w = 1920,
        h = 1080,
        blink = true,
        blink_timer = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
end

function M:update(dt)
    self.t = self.t + dt
    
    -- Blink the colon every 0.5 seconds
    self.blink_timer = self.blink_timer + dt
    if self.blink_timer >= 0.5 then
        self.blink_timer = self.blink_timer - 0.5
        self.blink = not self.blink
    end
end

function M:draw()
    -- Clear to dark background
    canvas.clear(15, 15, 25, 255)
    
    -- Get current time
    local time = os.date("*t")
    local hours = string.format("%02d", time.hour)
    local minutes = string.format("%02d", time.min)
    local seconds = string.format("%02d", time.sec)
    
    -- Colon (blinking)
    local colon = self.blink and ":" or " "
    
    -- Time string
    local time_str = hours .. colon .. minutes .. colon .. seconds
    
    -- Draw glow effect behind clock
    local clock_size = 120
    local tw, th = canvas.measure_text(time_str, clock_size)
    local cx = (self.w - tw) / 2
    local cy = (self.h - th) / 2
    
    -- Outer glow (multiple layers)
    for i = 3, 1, -1 do
        local alpha = 30 - i * 8
        local offset = i * 4
        canvas.draw_text(cx - offset, cy - offset/2, time_str, clock_size + offset, 0, 150, 255, alpha)
    end
    
    -- Main clock text
    canvas.draw_text(cx, cy, time_str, clock_size, 100, 200, 255, 255)
    
    -- Draw date below
    local date_str = os.date("%A, %B %d, %Y")
    local date_size = 32
    local dw, _ = canvas.measure_text(date_str, date_size)
    canvas.draw_text((self.w - dw) / 2, cy + th + 40, date_str, date_size, 150, 150, 180, 200)
    
    -- Draw decorative lines
    local line_width = tw + 100
    local line_y_top = cy - 30
    local line_y_bottom = cy + th + 20
    local line_x = (self.w - line_width) / 2
    
    canvas.fill_rect(line_x, line_y_top, line_width, 2, 60, 100, 150, 150)
    canvas.fill_rect(line_x, line_y_bottom, line_width, 2, 60, 100, 150, 150)
    
    -- Corner decorations
    local corner_size = 10
    -- Top left
    canvas.fill_rect(line_x, line_y_top - corner_size, 2, corner_size, 100, 200, 255, 200)
    canvas.fill_rect(line_x, line_y_top, corner_size, 2, 100, 200, 255, 200)
    -- Top right
    canvas.fill_rect(line_x + line_width - 2, line_y_top - corner_size, 2, corner_size, 100, 200, 255, 200)
    canvas.fill_rect(line_x + line_width - corner_size, line_y_top, corner_size, 2, 100, 200, 255, 200)
    -- Bottom left
    canvas.fill_rect(line_x, line_y_bottom, 2, corner_size, 100, 200, 255, 200)
    canvas.fill_rect(line_x, line_y_bottom, corner_size, 2, 100, 200, 255, 200)
    -- Bottom right
    canvas.fill_rect(line_x + line_width - 2, line_y_bottom, 2, corner_size, 100, 200, 255, 200)
    canvas.fill_rect(line_x + line_width - corner_size, line_y_bottom, corner_size, 2, 100, 200, 255, 200)
end

function M:save_state()
    return { t = self.t, blink = self.blink, blink_timer = self.blink_timer }
end

function M:load_state(state)
    self.t = state.t or self.t
    self.blink = state.blink or self.blink
    self.blink_timer = state.blink_timer or self.blink_timer
end

return M
