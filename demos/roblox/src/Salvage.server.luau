local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local StarterGui = game:GetService("StarterGui")
local Station = require(script.Parent.Station)

local ROUND_SECONDS = 90
local RESTART_SECONDS = 6
local PROMPT_DISTANCE = 10
local positions = {
    Vector3.new(4.4, 1, 25),
    Vector3.new(28, 1, 23),
    Vector3.new(32, 1, -25),
    Vector3.new(8, 1, -36),
    Vector3.new(-33, 1, -30),
    Vector3.new(-22, 1, -24),
}

StarterGui.ScreenOrientation = Enum.ScreenOrientation.Sensor
local station = Station.build()
local state = Instance.new("Folder")
state.Name = "StarSalvageState"
state:SetAttribute("Phase", "Waiting")
state:SetAttribute("Oxygen", ROUND_SECONDS)
state:SetAttribute("Charge", 0)
state:SetAttribute("ShortcutOpen", true)
state.Parent = ReplicatedStorage

local cells = Instance.new("Folder")
cells.Name = "Oxygen cells"
cells.Parent = station.arena
local carried = {}
local roundModels = {}
local deadline = 0
local charge = 0
local shutterOpen = false

local function promptFor(part, action, target)
    local prompt = Instance.new("ProximityPrompt")
    prompt.ActionText = action
    prompt.ObjectText = target
    prompt.HoldDuration = 0.22
    prompt.MaxActivationDistance = PROMPT_DISTANCE
    prompt.RequiresLineOfSight = false
    prompt.Parent = part
    return prompt
end

local function canInteract(player, part)
    local character = player.Character
    local root = character and character:FindFirstChild("HumanoidRootPart")
    local humanoid = character and character:FindFirstChildOfClass("Humanoid")
    return os.clock() < deadline and root and humanoid and humanoid.Health > 0
        and (root.Position - part.Position).Magnitude <= PROMPT_DISTANCE
end

local function dropCell(player)
    local cell = carried[player]
    if cell then
        carried[player] = nil
        cell.model.Parent = cells
        cell.prompt.Enabled = true
    end
    player:SetAttribute("CarryingBattery", false)
end

local function trackCharacter(player)
    player:SetAttribute("CarryingBattery", false)
    player.CharacterRemoving:Connect(function() dropCell(player) end)
end

Players.PlayerAdded:Connect(trackCharacter)
Players.PlayerRemoving:Connect(dropCell)
for _, player in Players:GetPlayers() do trackCharacter(player) end

local deliver = promptFor(station.reactor, "Insert cell", "Oxygen reactor")
deliver.Enabled = false
deliver.Triggered:Connect(function(player)
    local cell = carried[player]
    if not cell or not canInteract(player, station.reactor) then return end
    carried[player] = nil
    cell.model:Destroy()
    player:SetAttribute("CarryingBattery", false)
    charge += 1
    Station.setCharge(station, charge)
    state:SetAttribute("Charge", charge)
end)

local function doorwayOccupied()
    for _, player in Players:GetPlayers() do
        local root = player.Character and player.Character:FindFirstChild("HumanoidRootPart")
        if root then
            local offset = root.Position - station.gate.Position
            if math.abs(offset.X) < 7.8 and math.abs(offset.Z) < 2.4 then return true end
        end
    end
    return false
end

local function updateGate(open)
    if shutterOpen == open or (not open and doorwayOccupied()) then return end
    shutterOpen = open
    Station.setGate(station, open)
    state:SetAttribute("ShortcutOpen", open)
end

while true do
    for _, player in Players:GetPlayers() do dropCell(player) end
    for _, model in roundModels do model:Destroy() end
    table.clear(roundModels)
    charge = 0
    Station.setCharge(station, charge)
    updateGate(true)
    state:SetAttribute("Charge", charge)
    state:SetAttribute("Oxygen", ROUND_SECONDS)
    state:SetAttribute("Phase", "Waiting")
    if #Players:GetPlayers() == 0 then Players.PlayerAdded:Wait() end

    for index, position in positions do
        local model, casing = Station.battery(position, index, cells)
        table.insert(roundModels, model)
        local pickup = promptFor(casing, "Take cell", "Oxygen cell")
        pickup.Triggered:Connect(function(player)
            if carried[player] or model.Parent ~= cells or not canInteract(player, casing) then return end
            pickup.Enabled = false
            carried[player] = {model = model, prompt = pickup}
            model.Parent = nil
            player:SetAttribute("CarryingBattery", true)
        end)
    end

    deadline = os.clock() + ROUND_SECONDS
    deliver.Enabled = true
    state:SetAttribute("Phase", "Active")
    while charge < #positions and #Players:GetPlayers() > 0 do
        local left = math.max(0, deadline - os.clock())
        state:SetAttribute("Oxygen", math.ceil(left))
        if left == 0 then break end
        updateGate((ROUND_SECONDS - left) % 12 < 7)
        task.wait(0.2)
    end

    deadline = 0
    deliver.Enabled = false
    if #Players:GetPlayers() > 0 then
        state:SetAttribute("Phase", charge == #positions and "Won" or "Lost")
        task.wait(RESTART_SECONDS)
    end
end
