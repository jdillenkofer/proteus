-- Rube Goldberg Machine Manager
-- Renders multiple chambers in a tiled layout
-- Balls use GLOBAL coordinates and can overlap multiple chambers
-- Both chambers affect physics when ball overlaps their boundary

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        chambers = {},
        balls = {},
        t = 0,
        w = 1920,
        h = 1080,
        cols = 3,
        rows = 1,
        next_ball_id = 1,
        spawn_timer = 0,
        spawn_interval = 0.5,
        max_balls = 50,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    
    self:load_chambers()
    self:layout_chambers()
    
    for i, chamber in ipairs(self.chambers) do
        if chamber.init then
            chamber:init(chamber.viewport.w, chamber.viewport.h)
        end
    end
    
    for i = 1, 3 do
        self:spawn_random_ball()
    end
end

-- Create a ball at a random position (using GLOBAL coordinates)
function M:spawn_random_ball()
    if #self.chambers == 0 then return end
    
    -- Pick a random chamber from the TOP ROW (row 0)
    local top_chambers = {}
    for i, chamber in ipairs(self.chambers) do
        if chamber.viewport and (chamber.viewport.y or 0) < 10 then -- Assume row 0 is at y < 10
            table.insert(top_chambers, chamber)
        end
    end
    
    if #top_chambers == 0 then return end
    local chamber = top_chambers[math.random(1, #top_chambers)]
    local vp = chamber.viewport
    
    -- Random GLOBAL position within chamber
    local gx = vp.x + math.random(30, math.floor(vp.w - 30))
    local gy = vp.y + math.random(30, 80)
    
    local vx = math.random(-100, 100)
    local vy = math.random(0, 50)
    
    local colors = {
        {220, 80, 80},
        {80, 180, 220},
        {80, 220, 120},
        {220, 180, 80},
        {180, 80, 220},
    }
    local color = colors[math.random(1, #colors)]
    
    local ball = {
        id = self.next_ball_id,
        x = gx,  -- GLOBAL coordinates
        y = gy,
        vx = vx,
        vy = vy,
        radius = 10,
        color = color,
        active = true,
    }
    self.next_ball_id = self.next_ball_id + 1
    table.insert(self.balls, ball)
    return ball
end

-- Check if a ball's bounding circle overlaps a chamber
function M:ball_overlaps_chamber(ball, vp)
    local r = ball.radius
    return ball.x + r > vp.x and ball.x - r < vp.x + vp.w and
           ball.y + r > vp.y and ball.y - r < vp.y + vp.h
end

-- Get all chambers that a ball overlaps
function M:get_overlapping_chambers(ball)
    local overlapping = {}
    for i, chamber in ipairs(self.chambers) do
        if self:ball_overlaps_chamber(ball, chamber.viewport) then
            table.insert(overlapping, i)
        end
    end
    return overlapping
end

-- Layout chambers in a grid
function M:layout_chambers()
    local num = #self.chambers
    if num == 0 then return end
    
    if num <= 3 then
        self.cols = num
        self.rows = 1
    elseif num <= 6 then
        self.cols = 3
        self.rows = 2
    else
        self.cols = 4
        self.rows = math.ceil(num / 4)
    end
    
    local cell_w = self.w / self.cols
    local cell_h = self.h / self.rows
    
    for i, chamber in ipairs(self.chambers) do
        local col = (i - 1) % self.cols
        local row = math.floor((i - 1) / self.cols)
        
        chamber.viewport = {
            x = col * cell_w,
            y = row * cell_h,
            w = cell_w,
            h = cell_h,
            index = i,
        }
    end
end

-- Load chamber files
function M:load_chambers(ordered_files)
    self.chambers = {}
    
    local chamber_files = ordered_files or {
        "antigravity.lua",
        "tesla_coil.lua",
        "wind_tunnel.lua",
        "seesaw.lua",
        "pegs.lua",
        "funnel.lua",
        "stairs.lua",
        "trampoline.lua",
        "mixer.lua",
        "accelerator.lua",
        "splitter.lua",
        "conveyor.lua",
        "teleporter.lua",
        "magnet.lua",
        "bumper.lua",
        "pong.lua",
    }
    
    -- Shuffle chambers only if not restoring saved order
    if not ordered_files then
        for i = #chamber_files, 2, -1 do
            local j = math.random(i)
            chamber_files[i], chamber_files[j] = chamber_files[j], chamber_files[i]
        end
    end
    
    -- Store the order for saving
    self._chamber_order = chamber_files
    
    for i, filename in ipairs(chamber_files) do
        local path = "chambers/" .. filename
        local ok, chamber = pcall(dofile, path)
        if ok and chamber and chamber.new then
            local instance = chamber.new()
            instance._name = filename:gsub("%.lua$", "")
            instance._index = i
            table.insert(self.chambers, instance)
            print("Loaded chamber: " .. instance._name)
        else
            print("Failed to load chamber: " .. filename .. " - " .. tostring(chamber))
        end
    end
    
    print("Total chambers loaded: " .. #self.chambers)
end

function M:update(dt)
    self.t = self.t + dt
    
    -- Spawn new balls periodically
    self.spawn_timer = self.spawn_timer + dt
    if self.spawn_timer >= self.spawn_interval and #self.balls < self.max_balls then
        self.spawn_timer = 0
        self:spawn_random_ball()
    end
    
    -- 1. PHYSICS INTEGRATION
    -- Apply semi-implicit Euler integration (velocity then position).
    -- This runs exactly once per frame per ball in Global space.
    local GRAVITY = 400
    for _, ball in ipairs(self.balls) do
        if ball.active then
            ball.vy = ball.vy + GRAVITY * dt
            ball.x = ball.x + ball.vx * dt
            ball.y = ball.y + ball.vy * dt
        end
    end
    
    -- 2. CHAMBER INTERACTION
    -- Check balls against static obstacles within each chamber.
    for i, chamber in ipairs(self.chambers) do
        local vp = chamber.viewport
        local chamber_balls = {}
        
        -- Identify which balls are inside this chamber's bounding box
        for _, ball in ipairs(self.balls) do
            if ball.active and self:ball_overlaps_chamber(ball, vp) then
                -- Convert global ball state to LOCAL coordinates for the chamber
                -- Chambers only know about their own 0,0 origin
                local local_ball = {
                    _original = ball,
                    x = ball.x - vp.x,
                    y = ball.y - vp.y,
                    vx = ball.vx,
                    vy = ball.vy,
                    radius = ball.radius,
                    color = ball.color,
                    active = ball.active,
                }
                table.insert(chamber_balls, local_ball)
            end
        end
        
        -- Chamber resolves static collisions (walls, platforms) using local coords
        if chamber.update then
            chamber:update(dt, chamber_balls)
            
            -- Apply resolved positions/velocities back to global state
            for _, local_ball in ipairs(chamber_balls) do
                local orig = local_ball._original
                orig.x = local_ball.x + vp.x
                orig.y = local_ball.y + vp.y
                orig.vx = local_ball.vx
                orig.vy = local_ball.vy
                orig.active = local_ball.active
            end
        end
    end
    
    -- 3. GLOBAL BALL-BALL COLLISIONS
    -- Resolves dynamic collisions between balls.
    -- O(N^2) check is acceptable for low N (< 20).
    for i, ball in ipairs(self.balls) do
        if ball.active then
            for j = i + 1, #self.balls do
                local other = self.balls[j]
                if other.active then
                    local dx = other.x - ball.x
                    local dy = other.y - ball.y
                    local dist_sq = dx * dx + dy * dy
                    local min_dist = ball.radius + other.radius
                    
                    if dist_sq < min_dist * min_dist and dist_sq > 0 then
                        local dist = math.sqrt(dist_sq)
                        local nx = dx / dist
                        local ny = dy / dist
                        
                        -- Position Correction: Push balls apart equally
                        local overlap = (min_dist - dist) / 2
                        ball.x = ball.x - nx * overlap
                        ball.y = ball.y - ny * overlap
                        other.x = other.x + nx * overlap
                        other.y = other.y + ny * overlap
                        
                        -- Velocity Response: Elastic collision
                        local dvx = ball.vx - other.vx
                        local dvy = ball.vy - other.vy
                        local dvn = dvx * nx + dvy * ny
                        
                        -- Only separate if moving towards each other
                        if dvn > 0 then
                            ball.vx = ball.vx - dvn * nx
                            ball.vy = ball.vy - dvn * ny
                            other.vx = other.vx + dvn * nx
                            other.vy = other.vy + dvn * ny
                        end
                    end
                end
            end
        end
    end
    
    -- 4. GLOBAL BOUNDARIES
    -- Keep balls within the screen bounds.
    for _, ball in ipairs(self.balls) do
        if ball.active then
            -- Left edge (Bounce)
            if ball.x - ball.radius < 0 then
                ball.x = ball.radius
                ball.vx = math.abs(ball.vx) * 0.8
            end
            -- Right edge (Bounce)
            if ball.x + ball.radius > self.w then
                ball.x = self.w - ball.radius
                ball.vx = -math.abs(ball.vx) * 0.8
            end
            -- Top edge (Bounce)
            if ball.y - ball.radius < 0 then
                ball.y = ball.radius
                ball.vy = math.abs(ball.vy) * 0.8
            end
            -- Bottom edge (Despawn)
            if ball.y - ball.radius > self.h then
                ball.active = false
            end
        end
    end
    
    -- Remove inactive balls and spawn replacements
    local removed_count = 0
    for i = #self.balls, 1, -1 do
        if not self.balls[i].active then
            table.remove(self.balls, i)
            removed_count = removed_count + 1
        end
    end
    
    for i = 1, removed_count do
        if #self.balls < self.max_balls then
            self:spawn_random_ball()
        end
    end
end

function M:draw()
    canvas.clear(20, 20, 30, 255)
    
    -- Draw chamber backgrounds and decorations
    for i, chamber in ipairs(self.chambers) do
        local vp = chamber.viewport
        
        local bg_shade = 25 + (i % 3) * 5
        canvas.fill_rect(vp.x + 2, vp.y + 2, vp.w - 4, vp.h - 4, bg_shade, bg_shade, bg_shade + 8, 255)
        canvas.stroke_rect(vp.x + 2, vp.y + 2, vp.w - 4, vp.h - 4, 60, 60, 80, 255, 2)
        
        if chamber.draw then
            canvas.push_clip(vp.x, vp.y, vp.w, vp.h)
            chamber:draw(vp.x, vp.y, vp.w, vp.h)
            canvas.pop_clip()
        end
    end
    
    -- Draw all balls (using global coordinates)
    for _, ball in ipairs(self.balls) do
        if ball.active then
            -- Shadow
            canvas.fill_circle(ball.x + 3, ball.y + 3, ball.radius, 20, 20, 20, 80)
            -- Ball
            canvas.fill_circle(ball.x, ball.y, ball.radius, ball.color[1], ball.color[2], ball.color[3], 255)
            -- Highlight
            canvas.stroke_circle(ball.x, ball.y, ball.radius, 255, 255, 255, 80, 2)
        end
    end
end

-- State persistence
function M:save_state()
    local ball_states = {}
    for i, ball in ipairs(self.balls) do
        ball_states[i] = {
            x = ball.x, y = ball.y,
            vx = ball.vx, vy = ball.vy,
            radius = ball.radius,
            color = ball.color,
            active = ball.active,
        }
    end
    
    -- Save chamber states
    local chamber_states = {}
    for i, chamber in ipairs(self.chambers) do
        if chamber.save_state then
            chamber_states[i] = {
                name = chamber.name,
                viewport = chamber.viewport,
                state = chamber:save_state(),
            }
        end
    end
    
    return {
        t = self.t,
        w = self.w,
        h = self.h,
        balls = ball_states,
        next_ball_id = self.next_ball_id,
        spawn_timer = self.spawn_timer,
        chamber_states = chamber_states,
        chamber_order = self._chamber_order,
        cols = self.cols,
        rows = self.rows,
    }
end

function M:load_state(state)
    self.t = state.t or self.t
    self.w = state.w or self.w
    self.h = state.h or self.h
    self.next_ball_id = state.next_ball_id or self.next_ball_id
    self.spawn_timer = state.spawn_timer or 0
    
    -- Restore grid layout if available
    if state.cols then self.cols = state.cols end
    if state.rows then self.rows = state.rows end
    
    -- If chambers are empty (fresh module), load them from saved order
    if #self.chambers == 0 and state.chamber_order then
        self:load_chambers(state.chamber_order)
        self:layout_chambers()
        
        -- Initialize chambers
        for i, chamber in ipairs(self.chambers) do
            if chamber.init then
                chamber:init(chamber.viewport.w, chamber.viewport.h)
            end
        end
    end
    
    -- Restore chamber states
    if state.chamber_states and #self.chambers > 0 then
        for i, chamber in ipairs(self.chambers) do
            if state.chamber_states[i] then
                local saved = state.chamber_states[i]
                -- Restore viewport
                if saved.viewport then
                    chamber.viewport = saved.viewport
                end
                -- Restore internal state
                if chamber.load_state and saved.state then
                    chamber:load_state(saved.state)
                end
            end
        end
    end
    
    -- Restore balls
    if state.balls then
        self.balls = {}
        for _, bs in ipairs(state.balls) do
            table.insert(self.balls, {
                x = bs.x, y = bs.y,
                vx = bs.vx, vy = bs.vy,
                radius = bs.radius or 20,
                color = bs.color or {220, 80, 80},
                active = bs.active,
            })
        end
    end
end

return M
