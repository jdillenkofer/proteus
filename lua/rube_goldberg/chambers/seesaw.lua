-- Chamber: See-Saw
-- Tilting platforms that react to ball weight

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        seesaws = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.seesaws = {
        { cx = w * 0.5, cy = h * 0.4, length = w * 0.7, angle = 0, angular_vel = 0 },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    for _, s in ipairs(self.seesaws) do
        -- Apply gravity/damping to angle
        s.angular_vel = s.angular_vel - s.angle * 2 * dt -- Spring back to center
        s.angular_vel = s.angular_vel * 0.98 -- Damping
        s.angle = s.angle + s.angular_vel * dt
        
        -- Clamp angle
        if s.angle > 0.4 then s.angle = 0.4 s.angular_vel = 0 end
        if s.angle < -0.4 then s.angle = -0.4 s.angular_vel = 0 end
    end
    
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, s in ipairs(self.seesaws) do
            local cos_a = math.cos(s.angle)
            local sin_a = math.sin(s.angle)
            
            -- Endpoints
            local x1 = s.cx - cos_a * s.length * 0.5
            local y1 = s.cy - sin_a * s.length * 0.5
            local x2 = s.cx + cos_a * s.length * 0.5
            local y2 = s.cy + sin_a * s.length * 0.5
            
            -- Distance to line segment
            local dx = x2 - x1
            local dy = y2 - y1
            local len_sq = dx*dx + dy*dy
            local t_proj = math.max(0, math.min(1, ((ball.x - x1)*dx + (ball.y - y1)*dy) / len_sq))
            local proj_x = x1 + t_proj * dx
            local proj_y = y1 + t_proj * dy
            
            local dist_x = ball.x - proj_x
            local dist_y = ball.y - proj_y
            local dist = math.sqrt(dist_x*dist_x + dist_y*dist_y)
            
            local thickness = 8
            if dist < ball.radius + thickness then
                -- Normal (perpendicular, pointing "up" relative to tilt)
                local nx = -sin_a
                local ny = cos_a
                if dist_y < 0 then nx, ny = -nx, -ny end -- Ensure outward
                
                -- Push out
                local overlap = (ball.radius + thickness) - dist
                ball.x = ball.x + nx * overlap
                ball.y = ball.y + ny * overlap
                
                -- Bounce
                local dot = ball.vx * nx + ball.vy * ny
                if dot < 0 then
                    ball.vx = ball.vx - 1.5 * dot * nx
                    ball.vy = ball.vy - 1.5 * dot * ny
                end
                
                -- Apply torque to see-saw based on where ball landed
                local lever = (t_proj - 0.5) * s.length
                s.angular_vel = s.angular_vel + lever * 0.0005
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    for _, s in ipairs(self.seesaws) do
        local cos_a = math.cos(s.angle)
        local sin_a = math.sin(s.angle)
        
        local x1 = ox + s.cx - cos_a * s.length * 0.5
        local y1 = oy + s.cy - sin_a * s.length * 0.5
        local x2 = ox + s.cx + cos_a * s.length * 0.5
        local y2 = oy + s.cy + sin_a * s.length * 0.5
        
        canvas.draw_line(x1, y1, x2, y2, 150, 100, 50, 255, 12)
        canvas.fill_circle(ox + s.cx, oy + s.cy, 10, 100, 80, 40, 255)
    end
end

function M:save_state()
    return { seesaws = self.seesaws, t = self.t }
end

function M:load_state(state)
    if state then
        self.seesaws = state.seesaws or self.seesaws
        self.t = state.t or self.t
    end
end

return M

