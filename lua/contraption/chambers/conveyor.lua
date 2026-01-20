-- Chamber 9: Conveyor
-- Moving belts (resolution-independent)

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        belts = {},
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
    
    -- Proportional belt height (about 3% of chamber height)
    local belt_h = math.max(8, math.floor(h * 0.03))
    
    self.belts = {
        { x = w * 0.1, y = h * 0.3, w = w * 0.4, h = belt_h, speed = 100 * self.scale },
        { x = w * 0.5, y = h * 0.6, w = w * 0.4, h = belt_h, speed = -100 * self.scale },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, b in ipairs(self.belts) do
            -- Collision same as platforms
            if ball.x + ball.radius > b.x and ball.x - ball.radius < b.x + b.w then
                 local prev_y = ball.y - ball.vy * dt
                 
                 if ball.vy > 0 and 
                    ball.y + ball.radius >= b.y and 
                    prev_y + ball.radius <= b.y + b.h then
                     
                     -- Snap
                     ball.y = b.y - ball.radius
                     
                     -- Stop vertical
                     ball.vy = 0
                     
                     -- Apply horizontal conveyance
                     -- Linear interpolation towards belt speed (friction)
                     local friction = 5.0 * dt
                     ball.vx = ball.vx * (1 - friction) + b.speed * friction
                 end
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    local tread_spacing = math.max(10, math.floor(20 * self.scale))
    local line_width = math.max(1, math.floor(2 * self.scale))
    
    for _, b in ipairs(self.belts) do
        -- Draw belt rect
        canvas.fill_rect(ox + b.x, oy + b.y, b.w, b.h, 60, 60, 60, 255)
        
        -- Draw animated treads
        local offset = (self.t * b.speed) % tread_spacing
        for i = 0, b.w, tread_spacing do
            local x = i + offset
            if x > b.w then x = x - b.w end
            if x < 0 then x = x + b.w end -- handle negative speed
            
            -- don't draw if outside
            if x < b.w then
                 canvas.draw_line(ox + b.x + x, oy + b.y, ox + b.x + x, oy + b.y + b.h, 40, 40, 40, 255, line_width)
            end
        end
        
        -- Wheels at ends
        canvas.fill_circle(ox + b.x, oy + b.y + b.h/2, b.h/2 + 2, 30, 30, 30, 255)
        canvas.fill_circle(ox + b.x + b.w, oy + b.y + b.h/2, b.h/2 + 2, 30, 30, 30, 255)
    end
end

function M:save_state()
    return { belts = self.belts, t = self.t }
end

function M:load_state(state)
    if state then
        self.belts = state.belts or self.belts
        self.t = state.t or self.t
    end
end

return M

