-- Chamber 4: Stairs
-- Descending steps with dynamic movement and neon aesthetics

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        steps = {},
        t = 0,
        dir = 1, -- 1 for right, -1 for left
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    self.dir = math.random() < 0.5 and 1 or -1
    
    self.steps = {}
    local num_steps = math.random(6, 9)
    local total_w = w * 0.9
    local step_w = total_w / num_steps
    local step_h = (h * 0.7) / num_steps
    
    for i = 1, num_steps do
        local x
        if self.dir == 1 then
            x = (i - 1) * step_w
        else
            x = w - (i * step_w)
        end
        
        local is_booster = math.random() < 0.2
        
        table.insert(self.steps, {
            x_orig = x,
            y_orig = h * 0.15 + (i - 1) * step_h,
            x = x,
            y = h * 0.15 + (i - 1) * step_h,
            w = step_w + 5,
            h = 12,
            is_booster = is_booster,
            -- Move logic
            move_type = math.random(1, 4), -- 1: static, 2: horiz, 3: vert, 4: phase
            offset = math.random() * math.pi * 2,
            range = math.random(10, 30),
            speed = math.random() * 2 + 1,
        })
    end
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    -- Update step positions
    for _, s in ipairs(self.steps) do
        if s.move_type == 2 then
            s.x = s.x_orig + math.sin(self.t * s.speed + s.offset) * s.range
        elseif s.move_type == 3 then
            s.y = s.y_orig + math.cos(self.t * s.speed + s.offset) * s.range
        elseif s.move_type == 4 then
             -- Pulsing width or subtle shift
             s.x = s.x_orig + math.sin(self.t * s.speed) * 5
        end
    end
    
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, s in ipairs(self.steps) do
            -- Simple AABB for steps
            if ball.x + ball.radius * 0.5 >= s.x and ball.x - ball.radius * 0.5 <= s.x + s.w then
                 local prev_y = ball.y - ball.vy * dt
                 
                 if ball.vy > 0 and 
                    ball.y + ball.radius >= s.y and 
                    prev_y + ball.radius <= s.y + s.h then
                     
                     ball.y = s.y - ball.radius
                     
                     local bounce = s.is_booster and 1.8 or 0.8
                     ball.vy = -ball.vy * bounce
                     
                     -- Progression nudge
                     ball.vx = ball.vx + (self.dir * 60 * dt)
                     if math.abs(ball.vx) < 30 then ball.vx = self.dir * 30 end
                 end
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    for i, s in ipairs(self.steps) do
        local r, g, b = 100, 100, 100
        
        if s.is_booster then
            r, g, b = 255, 50, 150 -- Neon pink
        else
            r, g, b = 50, 150, 255 -- Neon blue
        end
        
        -- Glow under
        canvas.fill_rect(ox + s.x - 2, oy + s.y - 2, s.w + 4, s.h + 4, r, g, b, 40)
        
        -- Slab
        canvas.fill_rect(ox + s.x, oy + s.y, s.w, s.h, 20, 20, 30, 255)
        
        -- Top neon edge
        canvas.fill_rect(ox + s.x, oy + s.y, s.w, 3, r, g, b, 255)
        
        -- Subtle highlight
        if s.is_booster then
            local pulse = math.sin(self.t * 10) * 0.5 + 0.5
            canvas.stroke_rect(ox + s.x, oy + s.y, s.w, s.h, 255, 255, 255, 50 + 100 * pulse, 1)
        end
    end
end

function M:save_state()
    return { steps = self.steps, dir = self.dir, t = self.t }
end

function M:load_state(state)
    if state then
        self.steps = state.steps or self.steps
        self.dir = state.dir or self.dir
        self.t = state.t or self.t
    end
end

return M


