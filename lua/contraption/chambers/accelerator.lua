-- Chamber 8: Accelerator
-- Speed boost zones (resolution-independent)

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        boosters = {},
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
    
    self.boosters = {}
    
    local num_boosters = math.random(3, 5)
    for i = 1, num_boosters do
        local max_attempts = 10
        local attempts = 0
        local placed = false
        
        while not placed and attempts < max_attempts do
            attempts = attempts + 1
            
            -- Proportional size (20-30% of chamber dimensions)
            local bw = w * (0.15 + math.random() * 0.15)
            local bh = h * (0.15 + math.random() * 0.15)
            
            -- Proportional position (10-90% range)
            local bx = w * (0.1 + math.random() * 0.7)
            local by = h * (0.1 + math.random() * 0.7)
            
            -- Ensure within bounds
            if bx + bw > w * 0.9 then bx = w * 0.9 - bw end
            if by + bh > h * 0.9 then by = h * 0.9 - bh end
            
            -- Check overlap with existing boosters (proportional margin)
            local margin = math.min(w, h) * 0.02
            local overlap = false
            for _, other in ipairs(self.boosters) do
                if bx < other.x + other.w + margin and bx + bw + margin > other.x and
                   by < other.y + other.h + margin and by + bh + margin > other.y then
                    overlap = true
                    break
                end
            end
            
            if not overlap then
                -- Random direction
                local angle = math.random() * math.pi * 2
                local dx = math.cos(angle)
                local dy = math.sin(angle)
                
                -- Scale force proportionally
                local base_force = 600 + math.random() * 600
                
                table.insert(self.boosters, {
                    x = bx, y = by,
                    w = bw, h = bh,
                    dir_x = dx, dir_y = dy,
                    force = base_force * self.scale
                })
                placed = true
            end
        end
    end
end

function M:update(dt, balls)
    self.t = self.t + dt
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, b in ipairs(self.boosters) do
            -- AABB overlapping check (with radius)
            if ball.x + ball.radius > b.x and ball.x - ball.radius < b.x + b.w and
               ball.y + ball.radius > b.y and ball.y - ball.radius < b.y + b.h then
                
                -- Apply acceleration force (increased)
                ball.vx = ball.vx + b.dir_x * b.force * 5 * dt
                ball.vy = ball.vy + b.dir_y * b.force * 5 * dt
                
                -- Visual effect: color shift (optional/temporary)
                -- ball.color = {255, 255, 100} -- maybe too intrusive to change Perm?
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    local line_width = math.max(2, math.floor(4 * self.scale))
    local arrow_size = math.max(8, math.floor(15 * self.scale))
    
    for _, b in ipairs(self.boosters) do
        -- Draw zone
        local alpha = 100 + math.sin(self.t * 10) * 50
        canvas.fill_rect(ox + b.x, oy + b.y, b.w, b.h, 50, 255, 50, alpha)
        
        -- Center of box
        local cx = ox + b.x + b.w * 0.5
        local cy = oy + b.y + b.h * 0.5
        
        -- Calculate arrow length based on smaller dimension to fit inside
        local len = math.min(b.w, b.h) * 0.4
        
        -- Direction vector (normalize roughly)
        local mag = math.sqrt(b.dir_x^2 + b.dir_y^2)
        local nx = b.dir_x / mag
        local ny = b.dir_y / mag
        
        -- Start and End of arrow shaft
        local x1 = cx - nx * len
        local y1 = cy - ny * len
        local x2 = cx + nx * len
        local y2 = cy + ny * len
        
        canvas.draw_line(x1, y1, x2, y2, 255, 255, 255, 255, line_width)
        
        -- Draw arrowhead (simple rotation)
        -- Tangent direction (-ny, nx)
        local t1x = x2 - nx * arrow_size + ny * arrow_size * 0.5
        local t1y = y2 - ny * arrow_size - nx * arrow_size * 0.5
        
        local t2x = x2 - nx * arrow_size - ny * arrow_size * 0.5
        local t2y = y2 - ny * arrow_size + nx * arrow_size * 0.5
        
        canvas.draw_line(x2, y2, t1x, t1y, 255, 255, 255, 255, line_width)
        canvas.draw_line(x2, y2, t2x, t2y, 255, 255, 255, 255, line_width)
    end
end

function M:save_state()
    return { boosters = self.boosters, t = self.t }
end

function M:load_state(state)
    if state then
        self.boosters = state.boosters or self.boosters
        self.t = state.t or self.t
    end
end

return M

