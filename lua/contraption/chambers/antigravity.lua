-- Chamber 1: Antigravity
-- Regions that reverse or negate gravity (resolution-independent)

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        zones = {},
        t = 0,
        particles = {},
        scale = 1,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    self.zones = {}
    self.particles = {}
    
    -- Calculate scale factor (reference: 480x270 per chamber at 1920x1080 in 4x4 grid)
    self.scale = math.min(w / 480, h / 270)
    
    local num_zones = math.random(2, 3)
    for i = 1, num_zones do
        -- Proportional zone sizes (30-60% width, 20-40% height)
        local zw = w * (0.3 + math.random() * 0.3)
        local zh = h * (0.2 + math.random() * 0.2)
        table.insert(self.zones, {
            x = w * (0.1 + math.random() * (0.8 - zw/w)),
            y = h * ((i-0.5) / num_zones) - zh * 0.5,
            w = zw,
            h = zh,
            force = -600 * self.scale, -- Scale force with resolution
            color = {150, 200, 255},
        })
    end
    
    -- Initial particles (count scales with chamber size)
    local particle_count = math.floor(30 * self.scale)
    for i = 1, particle_count do
        table.insert(self.particles, self:create_particle())
    end
end

function M:create_particle()
    return {
        x = math.random(0, math.floor(self.w)),
        y = math.random(0, math.floor(self.h)),
        size = math.max(1, math.floor((2 + math.random() * 2) * self.scale)),
        speed = (20 + math.random() * 30) * self.scale,
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
    local corner_size = math.max(5, math.floor(10 * self.scale))
    local stroke_width = math.max(1, math.floor(2 * self.scale))
    
    for _, z in ipairs(self.zones) do
        -- Faint background glow
        local pulse = 0.8 + 0.2 * math.sin(self.t * 3)
        local r, g, b = table.unpack(z.color)
        canvas.fill_rect(ox + z.x, oy + z.y, z.w, z.h, r, g, b, math.floor(20 * pulse))
        
        -- Borders
        canvas.stroke_rect(ox + z.x, oy + z.y, z.w, z.h, r, g, b, 150, stroke_width)
        
        -- Corner accents (scaled)
        canvas.fill_rect(ox + z.x, oy + z.y, corner_size, stroke_width, 255, 255, 255, 200)
        canvas.fill_rect(ox + z.x, oy + z.y, stroke_width, corner_size, 255, 255, 255, 200)
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

