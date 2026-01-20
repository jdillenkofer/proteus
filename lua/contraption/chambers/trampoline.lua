-- Chamber 7: Trampoline
-- A bouncy platform that launches balls upward (resolution-independent)

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        pad = {},
        t = 0,
        scale = 1,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    -- Calculate scale factor (reference: 480x270 per chamber at 1920x1080 in 4x4 grid)
    self.scale = math.min(w / 480, h / 270)
    
    -- Proportional pad height (about 3.5% of chamber height)
    local pad_height = math.max(10, math.floor(h * 0.035))
    
    self.pad = {
        x = w * 0.2,
        y = h * 0.7,
        w = w * 0.6,
        h = pad_height,
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
                
                -- Cap max velocity to prevent explosion (scaled)
                local max_vel = 800 * self.scale
                local min_vel = 200 * self.scale
                if ball.vy < -max_vel then ball.vy = -max_vel end
                if ball.vy > -min_vel then ball.vy = -350 * self.scale end -- Minimum bounce
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    local p = self.pad
    local leg_width = math.max(2, math.floor(4 * self.scale))
    local bed_thickness = math.max(4, math.floor(8 * self.scale))
    local stroke_width = math.max(1, math.floor(2 * self.scale))
    local leg_offset = math.max(5, math.floor(10 * self.scale))
    
    -- Draw legs
    canvas.draw_line(ox + p.x + leg_offset, oy + p.y + leg_offset, ox + p.x + leg_offset, oy + h, 100, 100, 100, 255, leg_width)
    canvas.draw_line(ox + p.x + p.w - leg_offset, oy + p.y + leg_offset, ox + p.x + p.w - leg_offset, oy + h, 100, 100, 100, 255, leg_width)
    
    -- Draw elastic bed (animate slightly with time if impact?) 
    -- Keep simple for now
    canvas.fill_rect(ox + p.x, oy + p.y, p.w, bed_thickness, 50, 50, 200, 255)
    canvas.stroke_rect(ox + p.x, oy + p.y, p.w, bed_thickness, 100, 100, 255, 255, stroke_width)
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

