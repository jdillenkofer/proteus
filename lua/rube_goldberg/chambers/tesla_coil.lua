-- Chamber: Tesla Coil
-- Electric zaps that give balls sudden velocity kicks (resolution-independent)

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        coils = {},
        t = 0,
        zaps = {},
        scale = 1,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    self.zaps = {}
    
    -- Calculate scale factor (reference: 480x270 per chamber at 1920x1080 in 4x4 grid)
    self.scale = math.min(w / 480, h / 270)
    
    -- Proportional coil sizes (about 6% radius, 25% range)
    local coil_radius = math.min(w, h) * 0.06
    local coil_range = math.min(w, h) * 0.25
    
    self.coils = {
        { x = w * 0.3, y = h * 0.5, radius = coil_radius, range = coil_range, charge = 0 },
        { x = w * 0.7, y = h * 0.5, radius = coil_radius, range = coil_range, charge = 0.5 },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    -- Update coil charges
    for _, c in ipairs(self.coils) do
        c.charge = c.charge + dt * 0.8
    end
    
    -- Decay zaps
    for i = #self.zaps, 1, -1 do
        self.zaps[i].life = self.zaps[i].life - dt
        if self.zaps[i].life <= 0 then
            table.remove(self.zaps, i)
        end
    end
    
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, c in ipairs(self.coils) do
            local dx = ball.x - c.x
            local dy = ball.y - c.y
            local dist = math.sqrt(dx*dx + dy*dy)
            
            -- Zap if in range and charged
            if dist < c.range and dist > c.radius and c.charge >= 1.0 then
                c.charge = 0
                
                -- Create visual zap
                table.insert(self.zaps, {
                    x1 = c.x, y1 = c.y,
                    x2 = ball.x, y2 = ball.y,
                    life = 0.15
                })
                
                -- Apply electric kick (scaled)
                local nx = dx / dist
                local ny = dy / dist
                ball.vx = ball.vx + nx * 400 * self.scale
                ball.vy = ball.vy + ny * 400 * self.scale
            end
            
            -- Bounce off coil core
            if dist < c.radius + ball.radius and dist > 0 then
                local nx = dx / dist
                local ny = dy / dist
                local overlap = (c.radius + ball.radius) - dist
                ball.x = ball.x + nx * overlap
                ball.y = ball.y + ny * overlap
                
                local dot = ball.vx * nx + ball.vy * ny
                ball.vx = ball.vx - 1.5 * dot * nx
                ball.vy = ball.vy - 1.5 * dot * ny
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    local zap_width = math.max(2, math.floor(3 * self.scale))
    local rim_width = math.max(2, math.floor(3 * self.scale))
    local indicator_size = math.max(2, math.floor(4 * self.scale))
    
    -- Draw zaps first (behind coils)
    for _, z in ipairs(self.zaps) do
        local alpha = math.floor(z.life / 0.15 * 255)
        canvas.draw_line(ox + z.x1, oy + z.y1, ox + z.x2, oy + z.y2, 200, 200, 255, alpha, zap_width)
    end
    
    -- Draw coils
    for _, c in ipairs(self.coils) do
        -- Charge glow
        local glow = math.max(0, math.min(255, math.floor(c.charge * 100)))
        canvas.fill_circle(ox + c.x, oy + c.y, c.range, 100, 100, 255, glow)
        
        -- Core
        canvas.fill_circle(ox + c.x, oy + c.y, c.radius, 50, 50, 80, 255)
        canvas.stroke_circle(ox + c.x, oy + c.y, c.radius, 150, 150, 255, 255, rim_width)
        
        -- Charge indicator
        local arc = c.charge * math.pi * 2
        for i = 0, 3 do
            local a = self.t * 2 + i * math.pi / 2
            if a < arc then
                local px = ox + c.x + math.cos(a) * (c.radius + 5 * self.scale)
                local py = oy + c.y + math.sin(a) * (c.radius + 5 * self.scale)
                canvas.fill_circle(px, py, indicator_size, 255, 255, 100, 255)
            end
        end
    end
end

function M:save_state()
    return { coils = self.coils, t = self.t }
end

function M:load_state(state)
    if state then
        self.coils = state.coils or self.coils
        self.t = state.t or self.t
    end
end

return M

