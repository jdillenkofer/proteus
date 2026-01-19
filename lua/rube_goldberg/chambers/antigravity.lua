-- Chamber 1: Antigravity
-- Regions that reverse or negate gravity

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        zones = {},
        t = 0,
        particles = {},
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    self.zones = {}
    self.particles = {}
    
    local num_zones = math.random(2, 3)
    for i = 1, num_zones do
        local zw = math.random(math.floor(w * 0.3), math.floor(w * 0.6))
        local zh = math.random(math.floor(h * 0.2), math.floor(h * 0.4))
        table.insert(self.zones, {
            x = math.random(math.floor(w * 0.1), math.floor(w * 0.9 - zw)),
            y = h * ((i-0.5) / num_zones) - zh * 0.5,
            w = zw,
            h = zh,
            force = -600, -- Counteracts gravity (400) and adds lift
            color = {150, 200, 255},
        })
    end
    
    -- Initial particles
    for i = 1, 30 do
        table.insert(self.particles, self:create_particle())
    end
end

function M:create_particle()
    return {
        x = math.random(0, math.floor(self.w)),
        y = math.random(0, math.floor(self.h)),
        size = math.random(2, 4),
        speed = math.random(20, 50),
        life = math.random(),
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    -- Update particles
    for _, p in ipairs(self.particles) do
        p.y = p.y - p.speed * dt
        if p.y < 0 then p.y = self.h end
        p.life = (p.life + dt * 0.5) % 1.0
    end
    
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        local in_zone = false
        for _, z in ipairs(self.zones) do
            if ball.x >= z.x and ball.x <= z.x + z.w and
               ball.y >= z.y and ball.y <= z.y + z.h then
                
                -- Apply anti-gravity force
                ball.vy = ball.vy + z.force * dt
                
                -- Dampen downward velocity if moving too fast
                if ball.vy > 100 then ball.vy = ball.vy * 0.9 end
                
                -- Slight horizontal drift
                ball.vx = ball.vx + math.sin(self.t * 2 + z.x) * 20 * dt
                
                in_zone = true
            end
        end
        
        -- Add global walls (to keep it contained)
        if ball.x < 10 then ball.x = 10 ball.vx = math.abs(ball.vx) * 0.5 end
        if ball.x > self.w - 10 then ball.x = self.w - 10 ball.vx = -math.abs(ball.vx) * 0.5 end
    end
end

function M:draw(ox, oy, w, h)
    -- Draw particles
    for _, p in ipairs(self.particles) do
        local alpha = math.floor(math.sin(p.life * math.pi) * 150)
        canvas.fill_circle(ox + p.x, oy + p.y, p.size, 200, 230, 255, alpha)
    end

    -- Draw zones
    for _, z in ipairs(self.zones) do
        -- Faint background glow
        local pulse = 0.8 + 0.2 * math.sin(self.t * 3)
        local r, g, b = table.unpack(z.color)
        canvas.fill_rect(ox + z.x, oy + z.y, z.w, z.h, r, g, b, math.floor(20 * pulse))
        
        -- Borders
        canvas.stroke_rect(ox + z.x, oy + z.y, z.w, z.h, r, g, b, 150, 2)
        
        -- Corner accents
        canvas.fill_rect(ox + z.x, oy + z.y, 10, 2, 255, 255, 255, 200)
        canvas.fill_rect(ox + z.x, oy + z.y, 2, 10, 255, 255, 255, 200)
    end
end

function M:save_state()
    return { zones = self.zones, particles = self.particles, t = self.t }
end

function M:load_state(state)
    if state then
        self.zones = state.zones or self.zones
        self.particles = state.particles or self.particles
        self.t = state.t or self.t
    end
end

return M

