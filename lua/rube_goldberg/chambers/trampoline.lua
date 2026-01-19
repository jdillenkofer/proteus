-- Chamber 7: Trampoline
-- A bouncy platform that launches balls upward

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        pad = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.pad = {
        x = w * 0.2,
        y = h * 0.7,
        w = w * 0.6,
        h = 20,
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    if not balls then return end
    
    local p = self.pad
    
    for _, ball in ipairs(balls) do
        -- AABB Check
        if ball.x + ball.radius > p.x and ball.x - ball.radius < p.x + p.w then
            local prev_y = ball.y - ball.vy * dt
            
            -- Hitting top of pad
            if ball.vy > 0 and 
               ball.y + ball.radius >= p.y and 
               prev_y + ball.radius <= p.y + p.h then
                
                -- Snap
                ball.y = p.y - ball.radius
                
                -- Super bounce (restitution > 1.0 for trampoline effect)
                ball.vy = -ball.vy * 1.5
                
                -- Cap max velocity to prevent explosion
                if ball.vy < -800 then ball.vy = -800 end
                if ball.vy > -200 then ball.vy = -350 end -- Minimum bounce
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    local p = self.pad
    
    -- Draw legs
    canvas.draw_line(ox + p.x + 10, oy + p.y + 10, ox + p.x + 10, oy + h, 100, 100, 100, 255, 4)
    canvas.draw_line(ox + p.x + p.w - 10, oy + p.y + 10, ox + p.x + p.w - 10, oy + h, 100, 100, 100, 255, 4)
    
    -- Draw elastic bed (animate slightly with time if impact?) 
    -- Keep simple for now
    canvas.fill_rect(ox + p.x, oy + p.y, p.w, 8, 50, 50, 200, 255)
    canvas.stroke_rect(ox + p.x, oy + p.y, p.w, 8, 100, 100, 255, 255, 2)
end

function M:save_state()
    return { pad = self.pad, t = self.t }
end

function M:load_state(state)
    if state then
        self.pad = state.pad or self.pad
        self.t = state.t or self.t
    end
end

return M

