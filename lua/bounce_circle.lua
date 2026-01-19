-- Demo Lua script for Proteus LuaCanvas
-- Shows a bouncing circle animation

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        t = 0,
        x = 100.0,
        y = 100.0,
        vx = 200.0,
        vy = 150.0,
        radius = 50,
        w = 1920,
        h = 1080
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.x = w / 2
    self.y = h / 2
end

function M:update(dt)
    self.t = self.t + dt
    
    -- Update position
    self.x = self.x + self.vx * dt
    self.y = self.y + self.vy * dt
    
    -- Bounce off walls
    if self.x - self.radius < 0 then
        self.x = self.radius
        self.vx = -self.vx
    elseif self.x + self.radius > self.w then
        self.x = self.w - self.radius
        self.vx = -self.vx
    end
    
    if self.y - self.radius < 0 then
        self.y = self.radius
        self.vy = -self.vy
    elseif self.y + self.radius > self.h then
        self.y = self.h - self.radius
        self.vy = -self.vy
    end
end

function M:draw()
    -- Clear to dark background
    canvas.clear(20, 20, 30, 255)
    
    -- Draw bouncing circle with animated color
    local r = math.floor(128 + 127 * math.sin(self.t * 2))
    local g = math.floor(128 + 127 * math.sin(self.t * 3))
    local b = math.floor(128 + 127 * math.sin(self.t * 5))
    
    canvas.fill_circle(self.x, self.y, self.radius, r, g, b, 255)
    
    -- Draw border around circle
    canvas.stroke_circle(self.x, self.y, self.radius + 5, 255, 255, 255, 200, 3)
end

function M:save_state()
    return {
        t = self.t,
        w = self.w,
        h = self.h,
        x = self.x,
        y = self.y,
        vx = self.vx,
        vy = self.vy,
        radius = self.radius
    }
end

function M:load_state(state)
    self.t = state.t or self.t
    self.w = state.w or self.w
    self.h = state.h or self.h
    self.x = state.x or self.x
    self.y = state.y or self.y
    self.vx = state.vx or self.vx
    self.vy = state.vy or self.vy
    self.radius = state.radius or self.radius
end

return M
