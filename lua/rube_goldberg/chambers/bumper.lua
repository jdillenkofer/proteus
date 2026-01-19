-- Chamber 12: Bumper
-- Pinball style bouncy bumpers

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        bumpers = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.bumpers = {}
    
    -- Randomized non-overlapping placement
    local num_bumpers = math.random(2, 4)
    local attempts = 0
    while #self.bumpers < num_bumpers and attempts < 100 do
        attempts = attempts + 1
        
        local radius = math.random(20, 35)
        local x = math.random(radius + 20, w - radius - 20)
        local y = math.random(math.floor(h * 0.15), math.floor(h * 0.85))
        
        local overlap = false
        for _, b in ipairs(self.bumpers) do
            local dx = x - b.x
            local dy = y - b.y
            local dist = math.sqrt(dx*dx + dy*dy)
            -- Minimum separation: sum of radii + margin
            if dist < (radius + b.radius + 20) then
                overlap = true
                break
            end
        end
        
        if not overlap then
            table.insert(self.bumpers, {
                x = x,
                y = y,
                radius = radius,
                hit_timer = 0,
            })
        end
    end
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    -- Update flash timers
    for _, b in ipairs(self.bumpers) do
        if b.hit_timer > 0 then
            b.hit_timer = b.hit_timer - dt
        end
    end
    
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, b in ipairs(self.bumpers) do
            local dx = ball.x - b.x
            local dy = ball.y - b.y
            local dist = math.sqrt(dx*dx + dy*dy)
            local min_dist = b.radius + ball.radius
            
            if dist < min_dist then
                -- Collision!
                local nx = dx / dist
                local ny = dy / dist
                
                -- Move out
                local overlap = min_dist - dist
                ball.x = ball.x + nx * overlap
                ball.y = ball.y + ny * overlap
                
                -- Reflect with high restitution (super bounce)
                local dvx = ball.vx
                local dvy = ball.vy
                local dot = dvx * nx + dvy * ny
                
                -- Restitution > 1.0!
                local restitution = 2.0
                ball.vx = ball.vx - (1 + restitution) * dot * nx
                ball.vy = ball.vy - (1 + restitution) * dot * ny
                
                -- Trigger visual flash
                b.hit_timer = 0.2
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    for _, b in ipairs(self.bumpers) do
        -- Base color
        local r, g, blu = 200, 50, 50
        
        -- Flash when hit
        if b.hit_timer > 0 then
            r, g, blu = 255, 255, 100
        end
        
        -- Draw body
        canvas.fill_circle(ox + b.x, oy + b.y, b.radius, r, g, blu, 255)
        
        -- Draw rim
        canvas.stroke_circle(ox + b.x, oy + b.y, b.radius, 255, 255, 255, 255, 3)
        
        -- Detail rings
        canvas.stroke_circle(ox + b.x, oy + b.y, b.radius * 0.6, 255, 255, 255, 150, 2)
        canvas.stroke_circle(ox + b.x, oy + b.y, b.radius * 0.3, 255, 255, 255, 150, 2)
    end
end

function M:save_state()
    return { bumpers = self.bumpers, t = self.t }
end

function M:load_state(state)
    if state then
        self.bumpers = state.bumpers or self.bumpers
        self.t = state.t or self.t
    end
end

return M

