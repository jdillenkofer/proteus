-- Chamber 6: Splitter
-- A wedge that splits balls to left or right sides (resolution-independent)

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        wedge = {},
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
    
    -- Randomize wedge shape
    local top_x = w * (0.5 + 0.1 * math.random()) -- 0.4 to 0.6
    local top_y = h * (0.15 + 0.1 * math.random()) -- 0.15 to 0.25
    local spread = w * (0.2 + 0.1 * math.random()) -- width spread
    
    self.wedge = {
        top = { x = top_x, y = top_y },
        left = { x = top_x - spread, y = h * 0.6 },
        right = { x = top_x + spread, y = h * 0.6 },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt
    
    if not balls then return end
    
    -- Precompute walls with fixed normals
    local walls = {
        -- Left wall (top -> left)
        { 
            x1 = self.wedge.top.x, y1 = self.wedge.top.y, 
            x2 = self.wedge.left.x, y2 = self.wedge.left.y,
            nx = -0.8, ny = -0.6 -- Approx normal pointing up-left
        },
        -- Right wall (top -> right)
        { 
            x1 = self.wedge.top.x, y1 = self.wedge.top.y, 
            x2 = self.wedge.right.x, y2 = self.wedge.right.y,
            nx = 0.8, ny = -0.6 -- Approx normal pointing up-right
        },
    }
    
    -- Recalculate precise normals
    for _, w in ipairs(walls) do
        local dx = w.x2 - w.x1
        local dy = w.y2 - w.y1
        local len = math.sqrt(dx*dx + dy*dy)
        -- Rotate 90 deg: (-dy, dx) for left, (dy, -dx) for right?
        -- Left: dx < 0, dy > 0. Normal should be (-dy, dx)? -> (-pos, neg). No, we want (-pos, -neg).
        -- Let's just use the logic: Normal is perpendicular.
        if w.nx < 0 then -- Left wall
             w.nx = -dy / len
             w.ny = dx / len
        else -- Right wall
             w.nx = dy / len
             w.ny = -dx / len
        end
    end
    
    for _, ball in ipairs(balls) do
        for _, wall in ipairs(walls) do
            -- Distance to infinite line
            local dx = wall.x2 - wall.x1
            local dy = wall.y2 - wall.y1
            local len_sq = dx * dx + dy * dy
            
            -- Project ball onto line segment
            local t = ((ball.x - wall.x1) * dx + (ball.y - wall.y1) * dy) / len_sq
            t = math.max(0, math.min(1, t))
            
            local proj_x = wall.x1 + t * dx
            local proj_y = wall.y1 + t * dy
            
            local dist_sq = (ball.x - proj_x)^2 + (ball.y - proj_y)^2
            local min_dist = ball.radius + math.max(2, math.floor(5 * self.scale))
            
            if dist_sq < min_dist * min_dist then
                local dist = math.sqrt(dist_sq)
                
                -- Check if ball is on the "correct" side (using dot product with normal)
                local to_ball_x = ball.x - proj_x
                local to_ball_y = ball.y - proj_y
                local dot = to_ball_x * wall.nx + to_ball_y * wall.ny
                
                -- Only collide if ball is entering from the valid side or is very close
                -- Or simply enforce position: always push out along normal
                
                local overlap = min_dist - dist
                -- If inside (dot < 0), we need to push ALL the way out plus overlap
                if dot < 0 then overlap = min_dist + dist end -- Heuristic fix
                
                -- Better: Just ALWAYS push out along the fixed normal
                -- But we need to know how deep we are given the fixed normal
                -- Signed distance to line
                local signed_dist = (ball.x - wall.x1) * wall.nx + (ball.y - wall.y1) * wall.ny
                
                if signed_dist < min_dist then
                     local pen = min_dist - signed_dist
                     ball.x = ball.x + wall.nx * pen
                     ball.y = ball.y + wall.ny * pen
                     
                     -- Reflect velocity
                     local v_dot = ball.vx * wall.nx + ball.vy * wall.ny
                     if v_dot < 0 then
                        ball.vx = ball.vx - 1.5 * v_dot * wall.nx
                        ball.vy = ball.vy - 1.5 * v_dot * wall.ny
                     end
                end
            end
        end
        

    end
end

function M:draw(ox, oy, w, h)
    local line_width = math.max(5, math.floor(10 * self.scale))
    
    -- Draw wedge
    local top = self.wedge.top
    local left = self.wedge.left
    local right = self.wedge.right
    
    canvas.draw_line(ox + top.x, oy + top.y, ox + left.x, oy + left.y, 100, 200, 150, 255, line_width)
    canvas.draw_line(ox + top.x, oy + top.y, ox + right.x, oy + right.y, 100, 200, 150, 255, line_width)
end

function M:save_state()
    return { wedge = self.wedge, t = self.t }
end

function M:load_state(state)
    if state then
        self.wedge = state.wedge or self.wedge
        self.t = state.t or self.t
    end
end

return M

