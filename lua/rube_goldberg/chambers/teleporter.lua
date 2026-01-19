-- Chamber 10: Teleporter
-- Instant portal transitions with simplified visuals

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        portals = {},
        t = 0,
        cooldowns = {},
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    self.cooldowns = {}
    
    self.portals = {}
    
    local num_pairs = math.random(2, 3)
    local pair_colors = {
        { {50, 150, 255}, {50, 255, 200} }, -- Blue/Teal
        { {255, 150, 50}, {255, 200, 50} }, -- Orange/Yellow
        { {200, 50, 200}, {150, 50, 255} }, -- Purple/Indigo
    }
    
    for p = 1, num_pairs do
        local color_pair = pair_colors[p] or { {255, 255, 255}, {200, 200, 200} }
        
        -- Try to place a pair
        local pair = {}
        local pair_attempts = 0
        while #pair < 2 and pair_attempts < 100 do
            pair_attempts = pair_attempts + 1
            local radius = 22
            local x = math.random(radius + 15, w - radius - 15)
            local y = math.random(math.floor(h * 0.1), math.floor(h * 0.9))
            
            -- Check overlap with ALL portals (including current pair)
            local o = false
            for _, existing in ipairs(self.portals) do
                local d = math.sqrt((x - existing.x)^2 + (y - existing.y)^2)
                if d < (radius * 4) then o = true break end
            end
            for _, existing in ipairs(pair) do
                local d = math.sqrt((x - existing.x)^2 + (y - existing.y)^2)
                if d < (radius * 4) then o = true break end
            end
            
            if not o then
                table.insert(pair, { x = x, y = y, radius = radius, color = color_pair[#pair + 1] })
            end
        end
        
        if #pair == 2 then
            -- Link them
            local id1 = #self.portals + 1
            local id2 = #self.portals + 2
            pair[1].target = id2
            pair[2].target = id1
            table.insert(self.portals, pair[1])
            table.insert(self.portals, pair[2])
        end
    end
end

function M:update(dt, balls)
    self.t = self.t + dt
    if not balls then return end
    
    -- Cooldown tick
    for ball, timer in pairs(self.cooldowns) do
        self.cooldowns[ball] = timer - dt
        if self.cooldowns[ball] <= 0 then
            self.cooldowns[ball] = nil
        end
    end
    
    for _, ball in ipairs(balls) do
        if not self.cooldowns[ball] then
            for i, p in ipairs(self.portals) do
                local dx = ball.x - p.x
                local dy = ball.y - p.y
                local dist = math.sqrt(dx*dx + dy*dy)
                
                if dist < p.radius then
                    local target = self.portals[p.target]
                    if target then
                        -- TELEPORT: Move to target center
                        ball.x = target.x
                        ball.y = target.y
                        
                        -- CRITICAL: Offset the ball slightly in its current velocity direction 
                        -- to ensure it exits the target portal's radius immediately or doesn't trigger back.
                        -- However, a better way is to move it to the edge.
                        local vel_mag = math.sqrt(ball.vx*ball.vx + ball.vy*ball.vy)
                        if vel_mag > 0.1 then
                            local vnx = ball.vx / vel_mag
                            local vny = ball.vy / vel_mag
                            ball.x = ball.x + vnx * (target.radius + ball.radius + 2)
                            ball.y = ball.y + vny * (target.radius + ball.radius + 2)
                        else
                            -- If stationary, just push down
                            ball.y = ball.y + (target.radius + ball.radius + 2)
                        end
                        
                        -- Small cooldown just in case
                        self.cooldowns[ball] = 0.2
                        break
                    end
                end
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    for i, p in ipairs(self.portals) do
        local r, g, b = table.unpack(p.color)
        
        -- Static Glow
        local glow_alpha = math.floor(100 + 50 * math.sin(self.t * 5))
        canvas.fill_circle(ox + p.x, oy + p.y, p.radius * 1.2, r, g, b, 40)
        
        -- Dark Core
        canvas.fill_circle(ox + p.x, oy + p.y, p.radius, 10, 10, 20, 255)
        
        -- Sharp Rim
        canvas.stroke_circle(ox + p.x, oy + p.y, p.radius, r, g, b, 255, 3)
        canvas.stroke_circle(ox + p.x, oy + p.y, p.radius + 2, 255, 255, 255, 100, 1)
        
        -- Subtle energy orbit (constant speed, no pulses)
        local rot = self.t * 4
        for j = 1, 2 do
            local angle = rot + j * math.pi
            local ex = ox + p.x + math.cos(angle) * (p.radius + 3)
            local ey = oy + p.y + math.sin(angle) * (p.radius + 3)
            canvas.fill_circle(ex, ey, 4, 255, 255, 255, 180)
        end
    end
end

function M:save_state()
    return { portals = self.portals, t = self.t }
end

function M:load_state(state)
    if state then
        self.portals = state.portals or self.portals
        self.t = state.t or self.t
    end
end

return M

