-- Chamber: Wind Tunnel
-- Horizontal force zones that push balls sideways

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        fans = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.fans = {
        { x = 0, y = h * 0.2, w = w, h = h * 0.3, dir = 1, force = 1000 },
        { x = 0, y = h * 0.6, w = w, h = h * 0.3, dir = -1, force = 1000 },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, f in ipairs(self.fans) do
            if ball.x >= f.x and ball.x <= f.x + f.w and
               ball.y >= f.y and ball.y <= f.y + f.h then
                ball.vx = ball.vx + f.dir * f.force * dt
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    for _, f in ipairs(self.fans) do
        -- Zone background
        local r, g, b = 100, 150, 200
        if f.dir < 0 then r, g, b = 200, 150, 100 end
        canvas.fill_rect(ox + f.x, oy + f.y, f.w, f.h, r, g, b, 30)
        
        -- Animated wind lines
        local spacing = 40
        local offset = (self.t * f.dir * 100) % spacing
        for i = 0, math.floor(f.w / spacing) do
            local lx = f.x + i * spacing + offset
            if lx >= f.x and lx <= f.x + f.w then
                local ly = f.y + f.h * 0.5
                local len = 30 * f.dir
                canvas.draw_line(ox + lx, oy + ly - 10, ox + lx + len, oy + ly - 10, r, g, b, 150, 2)
                canvas.draw_line(ox + lx, oy + ly, ox + lx + len, oy + ly, r, g, b, 150, 2)
                canvas.draw_line(ox + lx, oy + ly + 10, ox + lx + len, oy + ly + 10, r, g, b, 150, 2)
            end
        end
    end
end

function M:save_state()
    return { fans = self.fans, t = self.t }
end

function M:load_state(state)
    if state then
        self.fans = state.fans or self.fans
        self.t = state.t or self.t
    end
end

return M

