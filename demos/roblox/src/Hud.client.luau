local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")

local player = Players.LocalPlayer
task.defer(function()
    local character = player.Character or player.CharacterAdded:Wait()
    local root = character:WaitForChild("HumanoidRootPart", 5)
    local camera = workspace.CurrentCamera
    if root and camera and camera.CameraType == Enum.CameraType.Custom then
        camera.CFrame = CFrame.lookAt(root.Position + Vector3.new(0, 6, 12), root.Position + Vector3.new(0, 0, -6))
    end
end)
local gui = Instance.new("ScreenGui")
gui.Name = "StationHud"
gui.ResetOnSpawn = false
gui.ScreenInsets = Enum.ScreenInsets.CoreUISafeInsets
gui.Parent = player:WaitForChild("PlayerGui")

local navy = Color3.fromRGB(16, 35, 52)
local cyan = Color3.fromRGB(101, 223, 201)
local amber = Color3.fromRGB(246, 173, 85)
local white = Color3.fromRGB(235, 245, 248)
local panel = Instance.new("Frame")
panel.Name = "Console"
panel.Size = UDim2.new(1, -20, 0, 92)
panel.Position = UDim2.new(0.5, 0, 0, 10)
panel.AnchorPoint = Vector2.new(0.5, 0)
panel.BackgroundColor3 = navy
panel.BackgroundTransparency = 0.06
panel.BorderSizePixel = 0
panel.Parent = gui
local limit = Instance.new("UISizeConstraint")
limit.MaxSize = Vector2.new(400, 92)
limit.Parent = panel
local corner = Instance.new("UICorner")
corner.CornerRadius = UDim.new(0, 8)
corner.Parent = panel
local stroke = Instance.new("UIStroke")
stroke.Color = cyan
stroke.Transparency = 0.45
stroke.Parent = panel

local function label(name, position, size, textSize, color, font)
    local text = Instance.new("TextLabel")
    text.Name = name
    text.Position = position
    text.Size = size
    text.BackgroundTransparency = 1
    text.Font = font
    text.TextSize = textSize
    text.TextColor3 = color
    text.TextXAlignment = Enum.TextXAlignment.Left
    text.TextWrapped = true
    text.Parent = panel
    return text
end

local title = label("Title", UDim2.fromOffset(12, 8), UDim2.new(0.43, -12, 0, 20), 13, cyan, Enum.Font.GothamBold)
title.Text = "STAR SALVAGE"
local oxygen = label("Oxygen", UDim2.new(0.43, 0, 0, 8), UDim2.new(0.25, 0, 0, 20), 15, white, Enum.Font.GothamBold)
local charge = label("Charge", UDim2.new(0.68, 0, 0, 8), UDim2.new(0.32, -12, 0, 20), 14, amber, Enum.Font.GothamBold)
charge.TextXAlignment = Enum.TextXAlignment.Right
local phase = label("Phase", UDim2.fromOffset(12, 34), UDim2.new(1, -24, 0, 17), 12, cyan, Enum.Font.GothamBold)
local nextStep = label("NextStep", UDim2.fromOffset(12, 54), UDim2.new(1, -24, 0, 34), 14, white, Enum.Font.Gotham)
nextStep.TextYAlignment = Enum.TextYAlignment.Top
oxygen.Text = "O₂  --"
charge.Text = "CELLS  --/6"
phase.Text = "CONNECTING"
nextStep.Text = "Waiting for station status."

local state = ReplicatedStorage:WaitForChild("StarSalvageState")
local function update()
    local currentPhase = state:GetAttribute("Phase")
    local o2 = state:GetAttribute("Oxygen")
    local power = state:GetAttribute("Charge")
    local shortcut = state:GetAttribute("ShortcutOpen")
    local carrying = player:GetAttribute("CarryingBattery")
    oxygen.Text = type(o2) == "number" and string.format("O₂  %ds", o2) or "O₂  --"
    oxygen.TextColor3 = type(o2) == "number" and o2 <= 15 and amber or white
    charge.Text = type(power) == "number" and string.format("CELLS  %d/6", power) or "CELLS  --/6"
    phase.TextColor3 = cyan
    if currentPhase == "Won" then
        phase.Text = "STATION SAVED"
        nextStep.Text = "All cells restored oxygen. A new run starts soon."
    elseif currentPhase == "Lost" then
        phase.Text = "OXYGEN DEPLETED"
        phase.TextColor3 = amber
        nextStep.Text = "A new run starts soon. Try the safe route."
    elseif currentPhase == "Waiting" then
        phase.Text = "WAITING"
        nextStep.Text = "Wait for the next salvage run."
    elseif currentPhase == "Active" then
        if power == 0 then
            phase.Text = "RESTORE THE OXYGEN REACTOR"
        else
            phase.Text = shortcut and "SHORTCUT OPEN" or "SHUTTER CLOSED  ·  SAFE ROUTE AVAILABLE"
            phase.TextColor3 = shortcut and cyan or amber
        end
        if carrying == true then
            nextStep.Text = "Insert the cell in the cyan oxygen reactor."
        elseif power == 0 then
            nextStep.Text = "Take an amber cell. Check beside the airlock first."
        else
            nextStep.Text = "Take another cell from the cargo or service bay."
        end
    else
        phase.Text = "STATUS UNKNOWN"
        nextStep.Text = "Waiting for station status."
    end
end
for _, attribute in ipairs({"Phase", "Oxygen", "Charge", "ShortcutOpen"}) do
    state:GetAttributeChangedSignal(attribute):Connect(update)
end
player:GetAttributeChangedSignal("CarryingBattery"):Connect(update)
update()
