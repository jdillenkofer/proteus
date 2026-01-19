-- Chamber 11: Magnet
-- Central attractor/repulsor

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        magnets = {},
        t = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0

    local type = math.random(1, 2) == 1 and "pull" or "push"
    
    self.magnets = {
        -- Central strong attractor
        { x = w * 0.5, y = h * 0.5, radius = 40, force = 9000000, type = type }, -- Scale force for gravity-like feel
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    if not balls then return end
    
    for _, ball in ipairs(balls) do
        for _, m in ipairs(self.magnets) do
            local dx = m.x - ball.x
            local dy = m.y - ball.y
            local dist_sq = dx*dx + dy*dy
            local dist = math.sqrt(dist_sq)
            
            -- Prevent singularity
            if dist < m.radius then dist = m.radius end
            if dist_sq < m.radius * m.radius then dist_sq = m.radius * m.radius end
            
            -- F = G * M / r^2
            -- Here, 'force' combines G and M constants
            local f = m.force / dist_sq
            
            if m.type == "push" then f = -f end
            
            -- Apply force (normalize dir vector first)
            local nx = dx / dist
            local ny = dy / dist
            
            ball.vx = ball.vx + nx * f * dt
            ball.vy = ball.vy + ny * f * dt
            
            -- If colliding with the physical magnet core, bounce
            if dist <= m.radius + ball.radius then
                -- Move out
                local overlap = (m.radius + ball.radius) - dist
                ball.x = ball.x - nx * overlap
                ball.y = ball.y - ny * overlap
                
                -- Bounce
                local dvx = ball.vx
                local dvy = ball.vy
                local dot = dvx * nx + dvy * ny
                
                ball.vx = ball.vx - 1.5 * dot * nx
                ball.vy = ball.vy - 1.5 * dot * ny
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    for _, m in ipairs(self.magnets) do
        -- Draw field lines
        local pulses = 5
        for i = 1, pulses do
            local offset = (self.t * 5 + i * (200/pulses)) % 200
            local radius = m.radius + offset
            local alpha = math.max(0, 255 - offset * 1.5)
            
            if m.type == "pull" then
                canvas.stroke_circle(ox + m.x, oy + m.y, radius, 100, 100, 255, alpha, 1)
            else
                canvas.stroke_circle(ox + m.x, oy + m.y, radius, 255, 100, 100, alpha, 1)
            end
        end
        
        -- Core
        if m.type == "pull" then
            canvas.fill_circle(ox + m.x, oy + m.y, m.radius, 50, 50, 200, 255)
        else
            canvas.fill_circle(ox + m.x, oy + m.y, m.radius, 200, 50, 50, 255)
        end
        
        -- Draw N (North/pull) or S (South/push) label on the magnet
        local label = m.type == "pull" and "N" or "S"
        local label_size = m.radius * 1.2
        local lw, lh = canvas.measure_text(label, label_size)
        canvas.draw_text(ox + m.x - lw * 0.5, oy + m.y - lh * 0.5, label, label_size, 255, 255, 255, 255)
    end
end

function M:save_state()
    return { magnets = self.magnets, t = self.t }
end

function M:load_state(state)
    if state then
        self.magnets = state.magnets or self.magnets
        self.t = state.t or self.t
    end
end

return M
