-- Chamber 3: Funnel
-- Angled walls forming a funnel shape (resolution-independent)

local M = {}
M.__index = M

local GRAVITY = 400

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        walls = {},
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
    
    -- Define walls as line segments
    self.walls = {
        { x1 = w * 0.05, y1 = h * 0.15, x2 = w * 0.35, y2 = h * 0.55 },
        { x1 = w * 0.95, y1 = h * 0.15, x2 = w * 0.65, y2 = h * 0.55 },
        { x1 = w * 0.25, y1 = h * 0.6, x2 = w * 0.4, y2 = h * 0.75 },
        { x1 = w * 0.75, y1 = h * 0.6, x2 = w * 0.6, y2 = h * 0.75 },
    }
end

-- Helper: distance from point to line segment
function M:point_line_dist(px, py, x1, y1, x2, y2)
    local dx = x2 - x1
    local dy = y2 - y1
    local len_sq = dx * dx + dy * dy
    if len_sq == 0 then return math.sqrt((px - x1)^2 + (py - y1)^2), x1, y1 end
    
    local t = math.max(0, math.min(1, ((px - x1) * dx + (py - y1) * dy) / len_sq))
    local proj_x = x1 + t * dx
    local proj_y = y1 + t * dy
    
    return math.sqrt((px - proj_x)^2 + (py - proj_y)^2), proj_x, proj_y
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    if not balls then return end
    
    -- Only apply obstacle collisions - gravity/position handled by manager
    for _, ball in ipairs(balls) do
        
        -- Wall collisions
        for _, wall in ipairs(self.walls) do
            local dist, proj_x, proj_y = self:point_line_dist(
                ball.x, ball.y, wall.x1, wall.y1, wall.x2, wall.y2
            )
            
            local wall_thick = math.max(2, math.floor(4 * self.scale))
            if dist < ball.radius + wall_thick then
                local nx = ball.x - proj_x
                local ny = ball.y - proj_y
                local len = math.sqrt(nx * nx + ny * ny)
                if len > 0 then
                    nx, ny = nx / len, ny / len
                else
                    nx, ny = 0, -1
                end
                
                -- Separate
                local overlap = ball.radius + wall_thick - dist
                ball.x = ball.x + nx * overlap
                ball.y = ball.y + ny * overlap
                
                -- Reflect velocity
                local dot = ball.vx * nx + ball.vy * ny
                ball.vx = ball.vx - 1.8 * dot * nx
                ball.vy = ball.vy - 1.8 * dot * ny
            end
        end
        

    end
end

function M:draw(ox, oy, w, h)
    local line_width = math.max(4, math.floor(8 * self.scale))
    
    -- Draw funnel walls
    for _, wall in ipairs(self.walls) do
        canvas.draw_line(
            ox + wall.x1, oy + wall.y1,
            ox + wall.x2, oy + wall.y2,
            90, 70, 110, 255, line_width
        )
    end
end

function M:save_state()
    return { walls = self.walls, t = self.t }
end

function M:load_state(state)
    if state then
        self.walls = state.walls or self.walls
        self.t = state.t or self.t
    end
end

return M

