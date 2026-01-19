-- Chamber: Pong
-- Auto-moving paddles that bounce balls back and forth

local M = {}
M.__index = M

function M.new()
    return setmetatable({
        w = 0,
        h = 0,
        paddles = {},
        t = 0,
        score_left = 0,
        score_right = 0,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.t = 0
    
    local paddle_width = 15
    local paddle_height = h * 0.25
    local margin = 20
    
    self.paddles = {
        -- Left paddle
        {
            x = margin,
            y = h * 0.5 - paddle_height * 0.5,
            w = paddle_width,
            h = paddle_height,
            target_y = h * 0.5,
            speed = 300,
            color = {100, 200, 255},
            side = "left",
        },
        -- Right paddle
        {
            x = w - margin - paddle_width,
            y = h * 0.5 - paddle_height * 0.5,
            w = paddle_width,
            h = paddle_height,
            target_y = h * 0.5,
            speed = 300,
            color = {255, 150, 100},
            side = "right",
        },
    }
end

function M:update(dt, balls)
    self.t = self.t + dt

    -- 1. Restore color of previously tracked ball (Cleanup)
    -- This handles the case where the ball leaves the chamber or changes ID
    if self.last_colored_ball and self.last_saved_color then
        -- Restore original color
        self.last_colored_ball.color = self.last_saved_color
        -- Clear references
        self.last_colored_ball = nil
        self.last_saved_color = nil
    end

    local target_ball = nil
    
    -- Try to find the existing target ball
    if self.target_ball_id then
        if balls then
            for _, b in ipairs(balls) do
                -- Check ID match (using internal _original table which has the ID)
                -- Note: balls passed to update are "local copies", we need to check the original ID
                if b._original and b._original.id == self.target_ball_id then
                    target_ball = b
                    break
                end
            end
        end
    end
    
    -- If no valid target (lost, or never set), pick a new one
    if not target_ball and balls and #balls > 0 then
        local cx, cy = self.w * 0.5, self.h * 0.5
        local min_dist = math.huge
        
        for _, ball in ipairs(balls) do
            local dx = ball.x - cx
            local dy = ball.y - cy
            local dist = dx*dx + dy*dy
            if dist < min_dist then
                min_dist = dist
                target_ball = ball
            end
        end
        
        -- Lock onto this ball
        if target_ball and target_ball._original then
            self.target_ball_id = target_ball._original.id
        end
    end
        
    -- 2. Disable Gravity and Apply Color for the Game Ball
    if target_ball then
        local GRAVITY = 400
        target_ball.vy = target_ball.vy - GRAVITY * dt
        target_ball.y = target_ball.y - GRAVITY * dt * dt
        
        -- Apply White Color (and save state for next frame cleanup)
        if target_ball._original then
            self.last_colored_ball = target_ball._original
            self.last_saved_color = target_ball._original.color
            target_ball._original.color = {255, 255, 255}
        end
    end
    
    -- 3. Update Paddles (Tracking the Game Ball)
    for _, paddle in ipairs(self.paddles) do
        -- Initialize reaction timer if missing
        paddle.reaction_timer = paddle.reaction_timer or 0
        paddle.reaction_timer = paddle.reaction_timer - dt
        
        -- Only update target position periodically (Reaction Time)
        if paddle.reaction_timer <= 0 then
            -- Reset timer (random delay 0.1s to 0.25s)
            paddle.reaction_timer = 0.1 + math.random() * 0.15
            
             if target_ball then
                 -- Predict where ball will be when it reaches paddle
                local time_to_paddle = 0
                if paddle.side == "left" and target_ball.vx < 0 then
                    time_to_paddle = (target_ball.x - paddle.x - paddle.w) / (-target_ball.vx)
                elseif paddle.side == "right" and target_ball.vx > 0 then
                    time_to_paddle = (paddle.x - target_ball.x) / target_ball.vx
                end
                
                -- If time is negative (moving away), return to center
                if time_to_paddle < 0 then
                    paddle.target_y = self.h * 0.5 - paddle.h * 0.5
                else
                    local predicted_y = target_ball.y + target_ball.vy * time_to_paddle
                    
                    -- Add "Human Error" (Perception Noise)
                    local error_margin = 50
                    local mistake = (math.random() * 2 - 1) * error_margin
                    
                    predicted_y = predicted_y + mistake
                    
                    paddle.target_y = math.max(0, math.min(self.h - paddle.h, predicted_y - paddle.h * 0.5))
                end
            else
                -- Return to center when no ball
                paddle.target_y = self.h * 0.5 - paddle.h * 0.5
            end
        end

        local diff = paddle.target_y - paddle.y
        local move = paddle.speed * dt
        
        if math.abs(diff) < move then
            paddle.y = paddle.target_y
        elseif diff > 0 then
            paddle.y = paddle.y + move
        else
            paddle.y = paddle.y - move
        end
        paddle.y = math.max(0, math.min(self.h - paddle.h, paddle.y))
    end
    
    if not balls then return end
    
    -- 4. Collision Logic
    for _, ball in ipairs(balls) do
        -- ... (paddle collision logic) ...
        for _, paddle in ipairs(self.paddles) do
            -- CCD: Check for tunneling (fast ball passing through paddle)
            local prev_x = ball.x - ball.vx * dt
            local prev_y = ball.y - ball.vy * dt
            
            local has_hit = false
            
            -- Check crossing of paddle face
            if paddle.side == "left" then
                local face_x = paddle.x + paddle.w
                -- Check if we crossed from right to left
                if prev_x >= face_x - ball.radius and ball.x <= face_x + ball.radius then
                     -- Calculate Y at crossing
                     local t = (face_x - prev_x) / (ball.x - prev_x)
                     local y_at_cross = prev_y + (ball.y - prev_y) * t
                     
                     -- Check strict vertical bounds (with radius tolerance)
                     if y_at_cross >= paddle.y - ball.radius and y_at_cross <= paddle.y + paddle.h + ball.radius then
                         has_hit = true
                         ball.x = face_x + ball.radius + 1 -- Push out
                     end
                end
            elseif paddle.side == "right" then
                local face_x = paddle.x
                -- Check if we crossed from left to right
                if prev_x <= face_x + ball.radius and ball.x >= face_x - ball.radius then
                     local t = (face_x - prev_x) / (ball.x - prev_x)
                     local y_at_cross = prev_y + (ball.y - prev_y) * t
                     
                     if y_at_cross >= paddle.y - ball.radius and y_at_cross <= paddle.y + paddle.h + ball.radius then
                         has_hit = true
                         ball.x = face_x - ball.radius - 1 -- Push out
                     end
                end
            end

            -- Fallback to standard overlap if CCD didn't trigger (for slow balls)
            if not has_hit then
                 local bx, by, br = ball.x, ball.y, ball.radius
                 local px, py, pw, ph = paddle.x, paddle.y, paddle.w, paddle.h
                 
                 local closest_x = math.max(px, math.min(bx, px + pw))
                 local closest_y = math.max(py, math.min(by, py + ph))
                 
                 local dx = bx - closest_x
                 local dy = by - closest_y
                 local dist_sq = dx * dx + dy * dy
                 
                 if dist_sq < br * br then
                     has_hit = true
                     -- Push out logic (simple)
                     local dist = math.sqrt(dist_sq)
                     if dist > 0 then
                         local nx = dx / dist
                         local ny = dy / dist
                         ball.x = ball.x + nx * (br - dist)
                         ball.y = ball.y + ny * (br - dist)
                     end
                 end
            end
            
            if has_hit then
                 if ball == target_ball or true then
                        -- Unified Pong Physics for ALL balls
                        local hit_pos = (ball.y - (paddle.y + paddle.h * 0.5)) / (paddle.h * 0.5)
                        hit_pos = math.max(-1, math.min(1, hit_pos))
                        
                        -- Flip X direction and speed up slightly
                        ball.vx = -ball.vx * 1.05
                        
                        -- Ensure minimum horizontal speed so it doesn't get stuck vertically
                        if  math.abs(ball.vx) < 300 then
                            ball.vx = (ball.vx < 0 and -300 or 300)
                        end
                        
                        -- Adjust Y velocity based on where it hit the paddle (English/Spin effect)
                        -- Hitting edges adds more vertical velocity
                        ball.vy = hit_pos * 400
                        
                        -- Cap max speed
                        local MAX_SPEED = 1000
                        local speed_sq = ball.vx*ball.vx + ball.vy*ball.vy
                        if speed_sq > MAX_SPEED*MAX_SPEED then
                            local scale = MAX_SPEED / math.sqrt(speed_sq)
                            ball.vx = ball.vx * scale
                            ball.vy = ball.vy * scale
                        end
                    end
            end
        end

        -- Top/bottom walls with hard clamping (prevent tunneling at high speeds)
        -- Only the game ball bounces off top/bottom to stay in play
        if ball == target_ball then
            -- Clamp Y position first to handle tunneling
            ball.y = math.max(ball.radius, math.min(self.h - ball.radius, ball.y))
            
            if ball.y <= ball.radius then
                ball.y = ball.radius
                ball.vy = math.abs(ball.vy) * 0.9
            end
            if ball.y >= self.h - ball.radius then
                ball.y = self.h - ball.radius
                ball.vy = -math.abs(ball.vy) * 0.9
            end
            
            -- Score detection: ball touched the back wall (paddle missed)
            -- Left wall: right player scores
            if ball.x - ball.radius <= 5 then
                self.score_right = self.score_right + 1
                -- Reset ball to center
                ball.x = self.w * 0.5
                ball.y = self.h * 0.5
                ball.vx = 300 + math.random() * 100
                ball.vy = (math.random() * 200 - 100)
            end
            -- Right wall: left player scores
            if ball.x + ball.radius >= self.w - 5 then
                self.score_left = self.score_left + 1
                -- Reset ball to center
                ball.x = self.w * 0.5
                ball.y = self.h * 0.5
                ball.vx = -300 - math.random() * 100
                ball.vy = (math.random() * 200 - 100)
            end
        end
    end
end

function M:draw(ox, oy, w, h)
    -- Center line (dashed)
    local dash_len = 20
    local gap = 15
    for y = 0, self.h, dash_len + gap do
        canvas.fill_rect(ox + self.w * 0.5 - 2, oy + y, 4, dash_len, 80, 80, 100, 150)
    end
    
    -- Draw score
    local score_size = 48
    local left_score_str = tostring(self.score_left)
    local right_score_str = tostring(self.score_right)
    
    -- Left score (blue side)
    local lw, _ = canvas.measure_text(left_score_str, score_size)
    canvas.draw_text(ox + self.w * 0.25 - lw * 0.5, oy + 20, left_score_str, score_size, 100, 200, 255, 200)
    
    -- Right score (orange side)
    local rw, _ = canvas.measure_text(right_score_str, score_size)
    canvas.draw_text(ox + self.w * 0.75 - rw * 0.5, oy + 20, right_score_str, score_size, 255, 150, 100, 200)
    
    -- Draw paddles
    for _, paddle in ipairs(self.paddles) do
        -- Glow effect
        canvas.fill_rect(
            ox + paddle.x - 3, oy + paddle.y - 3,
            paddle.w + 6, paddle.h + 6,
            paddle.color[1], paddle.color[2], paddle.color[3], 30
        )
        
        -- Main paddle
        canvas.fill_rect(
            ox + paddle.x, oy + paddle.y,
            paddle.w, paddle.h,
            paddle.color[1], paddle.color[2], paddle.color[3], 255
        )
        
        -- Highlight
        canvas.fill_rect(
            ox + paddle.x + 2, oy + paddle.y + 2,
            paddle.w - 4, 4,
            255, 255, 255, 100
        )
        
        -- Border
        canvas.stroke_rect(
            ox + paddle.x, oy + paddle.y,
            paddle.w, paddle.h,
            255, 255, 255, 150, 2
        )
    end
end

function M:save_state()
    local paddle_states = {}
    for i, p in ipairs(self.paddles) do
        paddle_states[i] = {
            x = p.x, y = p.y, w = p.w, h = p.h,
            target_y = p.target_y, speed = p.speed,
            color = p.color, side = p.side,
        }
    end
    return { paddles = paddle_states, t = self.t, target_ball_id = self.target_ball_id, score_left = self.score_left, score_right = self.score_right }
end

function M:load_state(state)
    if state then
        self.t = state.t or self.t
        self.target_ball_id = state.target_ball_id
        self.score_left = state.score_left or 0
        self.score_right = state.score_right or 0
        if state.paddles then
            for i, ps in ipairs(state.paddles) do
                if self.paddles[i] then
                    self.paddles[i].y = ps.y
                    self.paddles[i].target_y = ps.target_y
                end
            end
        end
    end
end

return M
