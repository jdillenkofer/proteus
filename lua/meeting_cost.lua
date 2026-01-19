-- Meeting Cost Counter for Proteus LuaCanvas
-- Renders FULLSCREEN - use hud_corner.frag shader to position in corner

local M = {}
M.__index = M

-- ============================================
-- CONFIGURATION - Adjust these values!
-- ============================================
local NUM_PERSONS = 4                    -- Number of people in the meeting
local YEARLY_SALARY_EUR = 65000          -- Average German developer salary (€)
local WORKING_HOURS_PER_YEAR = 2080      -- 40 hours/week * 52 weeks
-- ============================================

-- Calculated cost per second per person
local COST_PER_HOUR = YEARLY_SALARY_EUR / WORKING_HOURS_PER_YEAR
local COST_PER_SECOND = COST_PER_HOUR / 3600

function M.new()
    return setmetatable({
        t = 0,
        w = 1920,
        h = 1080,
        total_cost = 0,
        meeting_started = false,
    }, M)
end

function M:init(w, h)
    self.w = w
    self.h = h
    self.meeting_started = true
end

function M:update(dt)
    self.t = self.t + dt
    
    if self.meeting_started then
        -- Add cost for this frame (all persons)
        self.total_cost = self.total_cost + (COST_PER_SECOND * NUM_PERSONS * dt)
    end
end

function M:draw()
    -- Transparent background
    canvas.clear(0, 0, 0, 0)
    
    -- Full canvas dimensions for centering
    local cx = self.w / 2
    local cy = self.h / 2
    
    -- Background box (centered, covers most of canvas)
    local box_w = self.w * 0.9
    local box_h = self.h * 0.8
    local box_x = (self.w - box_w) / 2
    local box_y = (self.h - box_h) / 2
    
    canvas.fill_rect(box_x, box_y, box_w, box_h, 20, 15, 30, 240)
    canvas.stroke_rect(box_x, box_y, box_w, box_h, 255, 100, 100, 200, 4)
    
    -- Title (centered at top)
    local title = "MEETING COST"
    local title_size = 120
    local tw, _ = canvas.measure_text(title, title_size)
    canvas.draw_text(cx - tw/2, box_y + 50, title, title_size, 255, 120, 120, 255)
    
    -- Main cost display (large, centered)
    local cost_str = string.format("€ %.2f", self.total_cost)
    local cost_size = 280
    local cw, ch = canvas.measure_text(cost_str, cost_size)
    canvas.draw_text(cx - cw/2, cy - ch/2, cost_str, cost_size, 255, 220, 100, 255)
    
    -- Time elapsed (below cost)
    local minutes = math.floor(self.t / 60)
    local seconds = math.floor(self.t % 60)
    local time_str = string.format("Duration: %02d:%02d", minutes, seconds)
    local time_size = 80
    local timew, _ = canvas.measure_text(time_str, time_size)
    canvas.draw_text(cx - timew/2, cy + ch/2 + 60, time_str, time_size, 150, 150, 180, 255)
    
    -- Config info (bottom)
    local config_text = string.format("%d attendees × €%.0f/hour", NUM_PERSONS, COST_PER_HOUR)
    local config_size = 56
    local configw, _ = canvas.measure_text(config_text, config_size)
    canvas.draw_text(cx - configw/2, box_y + box_h - 150, config_text, config_size, 120, 140, 160, 200)
    
    -- Burn rate
    local rate_per_min = COST_PER_SECOND * NUM_PERSONS * 60
    local rate_str = string.format("Burning €%.2f per minute", rate_per_min)
    local rate_size = 52
    local ratew, _ = canvas.measure_text(rate_str, rate_size)
    canvas.draw_text(cx - ratew/2, box_y + box_h - 80, rate_str, rate_size, 255, 100, 100, 180)
end

function M:save_state()
    return { 
        t = self.t, 
        total_cost = self.total_cost,
        meeting_started = self.meeting_started 
    }
end

function M:load_state(state)
    self.t = state.t or self.t
    self.total_cost = state.total_cost or self.total_cost
    self.meeting_started = state.meeting_started or self.meeting_started
end

return M
