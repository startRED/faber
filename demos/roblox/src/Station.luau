local Lighting = game:GetService("Lighting")

local Station = {}
local V = Vector3.new
local rgb = Color3.fromRGB
local graphite = rgb(28, 43, 57)
local steel = rgb(84, 108, 129)
local shell = rgb(150, 173, 186)
local cyan = rgb(85, 202, 201)
local amber = rgb(248, 174, 85)
local red = rgb(251, 114, 102)

local function part(parent, name, size, position, color, solid, material)
    local object = Instance.new("Part")
    object.Name = name
    object.Anchored = true
    object.CanCollide = solid
    object.CanTouch = solid
    object.CanQuery = solid
    object.Material = material or Enum.Material.Metal
    object.Size = size
    object.Position = position
    object.Color = color
    object.Parent = parent
    return object
end

local function sign(parent, position, width, message, color)
    local plate = part(parent, "Sign", V(width, 1.9, 0.22), position, graphite, false)
    local surface = Instance.new("SurfaceGui")
    surface.Face = Enum.NormalId.Back
    surface.SizingMode = Enum.SurfaceGuiSizingMode.PixelsPerStud
    surface.PixelsPerStud = 48
    surface.LightInfluence = 0
    surface.Parent = plate
    local label = Instance.new("TextLabel")
    label.Size = UDim2.fromScale(0.94, 0.76)
    label.Position = UDim2.fromScale(0.03, 0.12)
    label.BackgroundTransparency = 1
    label.TextColor3 = color
    label.TextScaled = true
    label.Font = Enum.Font.GothamBold
    label.Text = message
    label.Parent = surface
    return label
end

local function workLight(parent, position, color)
    local strip = part(parent, "Work light", V(8, 0.22, 0.48), position, color, false, Enum.Material.Neon)
    local light = Instance.new("PointLight")
    light.Color = color:Lerp(Color3.new(1, 1, 1), 0.68)
    light.Brightness = 0.95
    light.Range = 24
    light.Parent = strip
end

function Station.build()
    Lighting.ClockTime = 21
    Lighting.Brightness = 2.7
    Lighting.Ambient = rgb(125, 139, 157)
    Lighting.OutdoorAmbient = rgb(110, 126, 148)

    local arena = Instance.new("Folder")
    arena.Name = "StarSalvage"
    arena.Parent = workspace
    part(arena, "Pressure deck", V(92, 2, 88), V(0, -1.01, -2), graphite, true, Enum.Material.Concrete)
    for x = -40, 40, 10 do
        for z = -42, 38, 10 do
            part(arena, "Deck panel", V(9.82, 0.08, 9.82), V(x, -0.02, z), (x + z) % 20 == 8 and steel or rgb(69, 90, 111), false)
        end
    end
    for _, side in {-1, 1} do
        local x = side * 46
        part(arena, "Pressure wall", V(1.2, 9, 88), V(x, 4.5, -2), steel, true)
        part(arena, "Side conduit", V(0.32, 0.26, 85), V(x - side * 0.75, 4, -2), cyan, false)
        for z = -38, 34, 12 do
            part(arena, "Side rib", V(0.9, 9, 0.72), V(x - side * 0.8, 4.5, z), graphite, false)
        end
    end
    for _, x in {-29, 29} do
        part(arena, "Airlock wall", V(34, 7, 1), V(x, 3.5, 42), steel, true)
        part(arena, "Airlock casing", V(0.9, 8, 1.6), V(x > 0 and 12 or -12, 4, 41.8), shell, true)
        part(arena, "Airlock side", V(1, 7, 14), V(x > 0 and 12 or -12, 3.5, 49), steel, true)
    end
    part(arena, "Airlock sill", V(24, 0.25, 2.1), V(0, 0.04, 41), steel, true)
    part(arena, "Airlock deck", V(24, 2, 14), V(0, -1, 49), graphite, true)
    part(arena, "Airlock door", V(24, 7, 1), V(0, 3.5, 56), shell, true)
    part(arena, "Observation wall", V(92, 2.1, 1), V(0, 1.05, -46), graphite, true)
    part(arena, "Observation beam", V(92, 2.7, 1.2), V(0, 10.3, -46), shell, true)
    for x = -39, 39, 13 do
        part(arena, "Observation mullion", V(0.7, 8, 1.1), V(x - 6.5, 5.8, -46), shell, false)
        local glass = part(arena, "Observation pane", V(12.3, 7, 0.25), V(x, 5.55, -46), rgb(80, 144, 179), true, Enum.Material.Glass)
        glass.Transparency = 0.78
    end
    part(arena, "Window end rib", V(0.7, 8, 1.1), V(45.5, 5.8, -46), shell, false)
    local planet = part(arena, "Planet", V(59, 59, 59), V(23, -4, -119), rgb(62, 108, 167), false, Enum.Material.SmoothPlastic)
    planet.Shape = Enum.PartType.Ball
    local stars = Random.new(45)
    for _ = 1, 18 do
        local size = stars:NextNumber(0.18, 0.5)
        local star = part(arena, "Distant star", V(size, size, size), V(stars:NextNumber(-110, 105), stars:NextNumber(8, 51), -155), shell, false, Enum.Material.Neon)
        star.Shape = Enum.PartType.Ball
    end

    for _, x in {-23, 23} do
        for _, z in {-29, 5, 26} do
            workLight(arena, V(x, 8, z), z == 26 and amber or cyan)
        end
    end
    for _, z in {-15, 13} do
        part(arena, "Overhead truss", V(92, 0.8, 0.9), V(0, 11.5, z), shell, false)
    end
    for _, x in {-10, 10} do
        part(arena, "Ceiling rail", V(0.7, 0.65, 44), V(x, 11.1, -4), graphite, false)
    end
    workLight(arena, V(0, 10, 1), cyan)
    for _, x in {-4.6, 4.6} do
        part(arena, "Reactor lane", V(0.22, 0.12, 23), V(x, 0.03, 18), cyan, false, Enum.Material.SmoothPlastic)
    end

    local spawn = Instance.new("SpawnLocation")
    spawn.Name = "Airlock spawn"
    spawn.Size = V(7, 0.3, 7)
    spawn.Position = V(0, 0.15, 32)
    spawn.Anchored = true
    spawn.Color = graphite
    spawn.Material = Enum.Material.Metal
    spawn.Duration = 0
    spawn.Neutral = true
    spawn.Parent = arena

    local foundation = part(arena, "Reactor plinth", V(0.5, 12, 12), V(0, 0.24, 0), steel, true)
    foundation.Shape = Enum.PartType.Cylinder
    foundation.Orientation = V(0, 0, 90)
    local reactor = part(arena, "Reactor", V(7, 3.4, 3.5), V(0, 2, 0), graphite, true)
    for _, x in {-3.6, 3.6} do
        part(arena, "Reactor brace", V(0.7, 4.4, 4), V(x, 2.5, 0), shell, true)
    end
    part(arena, "Reactor cap", V(8.2, 0.48, 4.4), V(0, 4.5, 0), shell, false)
    part(arena, "Reactor power strip", V(6.5, 0.14, 0.12), V(0, 3.63, 1.81), cyan, false, Enum.Material.Neon)
    sign(arena, V(0, 5.55, 0.6), 8.5, "OXYGEN REACTOR", cyan)
    local slots = {}
    for i = 1, 6 do
        slots[i] = part(arena, "Charge indicator", V(0.64, 2.45, 0.24), V((i - 3.5) * 0.95, 2.05, 1.81), steel, false)
    end

    part(arena, "Short route bulkhead", V(18, 7, 1.2), V(-37, 3.5, -11), steel, true)
    part(arena, "Short route bulkhead", V(29, 7, 1.2), V(-0.5, 3.5, -11), steel, true)
    part(arena, "Shutter header", V(13, 1.6, 1.8), V(-21.5, 7.05, -11), graphite, true)
    local gate = part(arena, "Service shutter", V(13, 6, 0.68), V(-21.5, 3, -11), graphite, true)
    local gateLamp = part(arena, "Shutter light", V(12, 0.15, 0.25), V(-21.5, 6.35, -9.95), cyan, false, Enum.Material.Neon)
    local gateLabel = sign(arena, V(-21.5, 8.3, -9.95), 13, "SHORTCUT OPEN", cyan)
    part(arena, "Route sign bracket", V(0.32, 5.2, 0.34), V(23, 2.6, -10.06), steel, false)
    sign(arena, V(18.5, 5.25, -9.95), 9, "SAFE ROUTE", cyan)
    sign(arena, V(29, 4.9, 14), 11, "CARGO BAY", amber)
    sign(arena, V(-25, 5.5, -36), 11, "SERVICE BAY", amber)
    for _, pos in {V(38, 1.1, 15), V(34.4, 1.1, 15), V(38, 1.1, 11.4), V(-39, 1.1, -39)} do
        part(arena, "Cargo container", V(3.4, 2.2, 3.4), pos, steel, true)
        part(arena, "Cargo lid", V(3.55, 0.22, 3.55), pos + V(0, 1.12, 0), graphite, false)
        part(arena, "Cargo marker", V(0.28, 1.3, 0.08), pos + V(-1.1, 0, 1.72), amber, false)
    end

    return {arena = arena, reactor = reactor, slots = slots, gate = gate, gateLamp = gateLamp, gateLabel = gateLabel}
end

function Station.setGate(station, open)
    station.gate.CanCollide = not open
    station.gate.CanTouch = not open
    station.gate.CanQuery = not open
    station.gate.Transparency = open and 1 or 0
    local tone = open and cyan or red
    station.gateLamp.Color = tone
    station.gateLabel.TextColor3 = tone
    station.gateLabel.Text = open and "SHORTCUT OPEN" or "SHUTTER CLOSED"
end

function Station.setCharge(station, charge)
    for i, slot in station.slots do
        local filled = i <= charge
        slot.Color = filled and cyan or steel
        slot.Material = filled and Enum.Material.Neon or Enum.Material.Metal
    end
end

function Station.battery(position, index, parent)
    local model = Instance.new("Model")
    model.Name = "Oxygen cell " .. index
    part(model, "Cradle", V(4.5, 0.28, 4.5), position + V(0, -0.9, 0), steel, false)
    local casing = part(model, "Cell", V(2.35, 1.8, 1.5), position, amber, true)
    for _, side in {-1, 1} do
        part(model, "Cell end cap", V(0.34, 1.95, 1.66), position + V(side * 1.11, 0, 0), graphite, false)
        part(model, "Cell contact", V(0.36, 0.15, 0.42), position + V(side * 0.68, 0.98, 0), shell, false)
    end
    part(model, "Cell charge stripe", V(1.46, 0.23, 0.13), position + V(0, 0.08, 0.78), rgb(255, 218, 145), false, Enum.Material.Neon)
    model.Parent = parent
    return model, casing
end

return Station
