-- Chamber 8: Accelerator
-- Speed boost zones

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        boosters = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.boosters = {}
    
    local num_boosters = math.random(3, 5)
    for i = 1, num_boosters do
        local max_attempts = 10
        local attempts = 0
        local placed = false
        
        while not placed and attempts < max_attempts do
            attempts = attempts + 1
            
            -- Random size
            local bw = math.random(40, math.floor(w * 0.3))
            local bh = math.random(40, math.floor(h * 0.3))
            
            -- Random position (keep somewhat away from edges)
            local bx = math.random(math.floor(w * 0.1), math.floor(w * 0.9 - bw))
            local by = math.random(math.floor(h * 0.1), math.floor(h * 0.9 - bh))
            
            -- Check overlap with existing boosters
            local overlap = false
            for _, other in ipairs(self.boosters) do
                if bx < other.x + other.w + 10 and bx + bw + 10 > other.x and
                   by < other.y + other.h + 10 and by + bh + 10 > other.y then
                    overlap = true
                    break
                end
            end
            
            if not overlap then
                -- Random direction
                local angle = math.random() * math.pi * 2
                local dx = math.cos(angle)
                local dy = math.sin(angle)
                
                table.insert(self.boosters, {
                    x = bx, y = by,
                    w = bw, h = bh,
                    dir_x = dx, dir_y = dy,
                    force = math.random(600, 1200)
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
        
        canvas.draw_line(x1, y1, x2, y2, 255, 255, 255, 255, 4)
        
        -- Draw arrowhead (simple rotation)
        -- Tangent direction (-ny, nx)
        local arrow_size = 15
        local t1x = x2 - nx * arrow_size + ny * arrow_size * 0.5
        local t1y = y2 - ny * arrow_size - nx * arrow_size * 0.5
        
        local t2x = x2 - nx * arrow_size - ny * arrow_size * 0.5
        local t2y = y2 - ny * arrow_size + nx * arrow_size * 0.5
        
        canvas.draw_line(x2, y2, t1x, t1y, 255, 255, 255, 255, 4)
        canvas.draw_line(x2, y2, t2x, t2y, 255, 255, 255, 255, 4)
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

