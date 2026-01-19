-- Chamber 2: Pegs (Plinko style)
-- Grid of pegs for balls to bounce through

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        pegs = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    self.pegs = {}
    
    -- Randomized peg placement
    local num_pegs = math.random(15, 25)
    local attempts = 0
    
    while #self.pegs < num_pegs and attempts < 1000 do
        attempts = attempts + 1
        
        local r = 12
        local margin = 40
        local x = math.random(margin, w - margin)
        local y = math.random(math.floor(h * 0.15), math.floor(h * 0.85))
        
        -- Overlap check: dist must be > peg_diam + ball_diam + margin
        local min_sep = r * 2 + 22 -- 46
        local overlap = false
        for _, p in ipairs(self.pegs) do
            local dist = math.sqrt((x - p.x)^2 + (y - p.y)^2)
            if dist < min_sep then 
                overlap = true
                break
            end
        end
        
        if not overlap then
             table.insert(self.pegs, { x = x, y = y, radius = r })
        end
    end
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    if not balls then return end
    
    -- Only apply obstacle collisions - gravity/position handled by manager
    for _, ball in ipairs(balls) do
        
        -- Peg collisions
        for _, peg in ipairs(self.pegs) do
            local dx = ball.x - peg.x
            local dy = ball.y - peg.y
            local dist = math.sqrt(dx * dx + dy * dy)
            local min_dist = ball.radius + peg.radius
            
            if dist < min_dist and dist > 0 then
                local nx = dx / dist
                local ny = dy / dist
                
                -- Separate
                local overlap = min_dist - dist
                ball.x = ball.x + nx * overlap
                ball.y = ball.y + ny * overlap
                
                -- Reflect velocity
                local dot = ball.vx * nx + ball.vy * ny
                ball.vx = ball.vx - 1.5 * dot * nx
                ball.vy = ball.vy - 1.5 * dot * ny
                
                -- Add randomness
                ball.vx = ball.vx + (math.random() - 0.5) * 30
            end
        end
        

    end
end

function M:draw(ox, oy, w, h)
    -- Draw pegs with pulsing glow
    for i, peg in ipairs(self.pegs) do
        local pulse = math.sin(self.t * 3 + i * 0.5) * 0.2 + 0.8
        local r = math.floor(100 * pulse)
        local g = math.floor(80 * pulse)
        local b = math.floor(140 * pulse)
        
        canvas.fill_circle(ox + peg.x, oy + peg.y, peg.radius, r, g, b, 255)
        canvas.stroke_circle(ox + peg.x, oy + peg.y, peg.radius, r + 40, g + 30, b + 30, 200, 2)
    end
end

function M:save_state()
    return {
        pegs = self.pegs,
        t = self.t,
    }
end

function M:load_state(state)
    if state then
        self.pegs = state.pegs or self.pegs
        self.t = state.t or self.t
    end
end

return M

