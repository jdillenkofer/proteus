-- Analog Clock for Proteus LuaCanvas
-- Classic analog clock with hour, minute, and second hands

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        t = 0,
        w = 1920,
        h = 1080,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
end

function M:update(dt)
    self.t = self.t + dt
end

function M:draw()
    -- Clear to dark background
    canvas.clear(20, 25, 35, 255)
    
    -- Clock center and radius
    local cx = self.w / 2
    local cy = self.h / 2
    local radius = math.min(self.w, self.h) * 0.35
    
    -- Get current time
    local time = os.date("*t")
    local hours = time.hour % 12
    local minutes = time.min
    local seconds = time.sec
    
    -- Draw outer glow
    for i = 5, 1, -1 do
        local alpha = 15 - i * 2
        canvas.stroke_circle(cx, cy, radius + i * 3, 80, 120, 180, alpha, 8)
    end
    
    -- Draw clock face background
    canvas.fill_circle(cx, cy, radius, 30, 35, 50, 255)
    
    -- Draw outer ring
    canvas.stroke_circle(cx, cy, radius, 100, 130, 180, 255, 4)
    canvas.stroke_circle(cx, cy, radius - 8, 60, 80, 120, 150, 2)
    
    -- Draw hour markers
    for i = 0, 11 do
        local angle = (i / 12) * math.pi * 2 - math.pi / 2
        local inner_r = radius - 25
        local outer_r = radius - 10
        
        local x1 = cx + math.cos(angle) * inner_r
        local y1 = cy + math.sin(angle) * inner_r
        local x2 = cx + math.cos(angle) * outer_r
        local y2 = cy + math.sin(angle) * outer_r
        
        -- Major hour markers (thicker for 12, 3, 6, 9)
        if i % 3 == 0 then
            canvas.draw_line(x1, y1, x2, y2, 200, 220, 255, 255, 4)
        else
            canvas.draw_line(x1, y1, x2, y2, 150, 170, 200, 200, 2)
        end
    end
    
    -- Draw minute markers
    for i = 0, 59 do
        if i % 5 ~= 0 then  -- Skip where hour markers are
            local angle = (i / 60) * math.pi * 2 - math.pi / 2
            local inner_r = radius - 15
            local outer_r = radius - 10
            
            local x1 = cx + math.cos(angle) * inner_r
            local y1 = cy + math.sin(angle) * inner_r
            local x2 = cx + math.cos(angle) * outer_r
            local y2 = cy + math.sin(angle) * outer_r
            
            canvas.draw_line(x1, y1, x2, y2, 80, 100, 140, 150, 1)
        end
    end
    
    -- Draw hour numbers
    local num_radius = radius - 45
    local num_size = 36
    for i = 1, 12 do
        local angle = (i / 12) * math.pi * 2 - math.pi / 2
        local num_str = tostring(i)
        local nw, nh = canvas.measure_text(num_str, num_size)
        local nx = cx + math.cos(angle) * num_radius - nw / 2
        local ny = cy + math.sin(angle) * num_radius - nh / 2
        canvas.draw_text(nx, ny, num_str, num_size, 180, 200, 240, 255)
    end
    
    -- Calculate hand angles (in radians, 0 = 12 o'clock position)
    local second_angle = (seconds / 60) * math.pi * 2 - math.pi / 2
    local minute_angle = ((minutes + seconds / 60) / 60) * math.pi * 2 - math.pi / 2
    local hour_angle = ((hours + minutes / 60) / 12) * math.pi * 2 - math.pi / 2
    
    -- Draw hour hand (shortest, thickest)
    local hour_len = radius * 0.5
    local hx = cx + math.cos(hour_angle) * hour_len
    local hy = cy + math.sin(hour_angle) * hour_len
    -- Shadow
    canvas.draw_line(cx + 2, cy + 2, hx + 2, hy + 2, 0, 0, 0, 80, 10)
    -- Hand
    canvas.draw_line(cx, cy, hx, hy, 200, 210, 230, 255, 8)
    
    -- Draw minute hand (longer, medium thickness)
    local minute_len = radius * 0.7
    local mx = cx + math.cos(minute_angle) * minute_len
    local my = cy + math.sin(minute_angle) * minute_len
    -- Shadow
    canvas.draw_line(cx + 2, cy + 2, mx + 2, my + 2, 0, 0, 0, 80, 6)
    -- Hand
    canvas.draw_line(cx, cy, mx, my, 180, 200, 240, 255, 5)
    
    -- Draw second hand (longest, thinnest, red)
    local second_len = radius * 0.85
    local sx = cx + math.cos(second_angle) * second_len
    local sy = cy + math.sin(second_angle) * second_len
    -- Tail (opposite direction)
    local tail_len = radius * 0.15
    local tx = cx - math.cos(second_angle) * tail_len
    local ty = cy - math.sin(second_angle) * tail_len
    -- Shadow
    canvas.draw_line(tx + 1, ty + 1, sx + 1, sy + 1, 0, 0, 0, 60, 3)
    -- Hand
    canvas.draw_line(tx, ty, sx, sy, 255, 80, 80, 255, 2)
    
    -- Draw center cap
    canvas.fill_circle(cx, cy, 12, 80, 90, 110, 255)
    canvas.fill_circle(cx, cy, 8, 255, 100, 100, 255)
    canvas.stroke_circle(cx, cy, 12, 120, 140, 180, 200, 2)
end

function M:save_state()
    return { t = self.t }
end

function M:load_state(state)
    self.t = state.t or self.t
end

return M
