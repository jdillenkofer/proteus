-- Chamber 5: Mixer
-- Spinning blades that mix balls around

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        blades = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.blades = {
        { cx = w * 0.3, cy = h * 0.5, len = 60, speed = 3 },
        { cx = w * 0.7, cy = h * 0.5, len = 60, speed = -2.5 },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        -- Blade collisions
        for _, b in ipairs(self.blades) do
            local angle = self.t * b.speed
            local s, c = math.sin(angle), math.cos(angle)
            
            -- Line segment for blade (centered at cx, cy)
            local x1 = b.cx - c * b.len
            local y1 = b.cy - s * b.len
            local x2 = b.cx + c * b.len
            local y2 = b.cy + s * b.len
            
            -- Distance to blade line
            local dx = x2 - x1
            local dy = y2 - y1
            local len_sq = dx * dx + dy * dy
            local t = math.max(0, math.min(1, ((ball.x - x1) * dx + (ball.y - y1) * dy) / len_sq))
            local proj_x = x1 + t * dx
            local proj_y = y1 + t * dy
            
            local dist_sq = (ball.x - proj_x)^2 + (ball.y - proj_y)^2
            local thick = 8
            
            if dist_sq < (ball.radius + thick)^2 then
                -- Deflect ball
                local nx = ball.x - proj_x
                local ny = ball.y - proj_y
                local len = math.sqrt(nx * nx + ny * ny)
                if len > 0 then nx, ny = nx/len, ny/len else nx=0; ny=-1 end
                
                -- Push out
                local overlap = (ball.radius + thick) - len
                ball.x = ball.x + nx * overlap
                ball.y = ball.y + ny * overlap
                
                -- Add velocity from spinner
                local spin_vel = b.speed * 100
                ball.vx = ball.vx + nx * 50 - s * spin_vel * 0.5
                ball.vy = ball.vy + ny * 50 + c * spin_vel * 0.5
            end
        end
        

    end
end

function M:draw(ox, oy, w, h)
    -- Draw blades
    for _, b in ipairs(self.blades) do
        local angle = self.t * b.speed
        local s, c = math.sin(angle), math.cos(angle)
        
        local x1 = ox + b.cx - c * b.len
        local y1 = oy + b.cy - s * b.len
        local x2 = ox + b.cx + c * b.len
        local y2 = oy + b.cy + s * b.len
        
        canvas.draw_line(x1, y1, x2, y2, 200, 100, 100, 255, 16)
        canvas.fill_circle(ox + b.cx, oy + b.cy, 10, 150, 150, 150, 255)
    end
end

function M:save_state()
    return { blades = self.blades, t = self.t }
end

function M:load_state(state)
    if state then
        self.blades = state.blades or self.blades
        self.t = state.t or self.t
    end
end

return M

