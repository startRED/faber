# Star Salvage

Recover six oxygen cells before the station runs out of air. The airlock has an easy first pickup. Carry cells one at a time to the cyan reactor, then explore the cargo and service bays. A shutter periodically closes the short route to the service bay. Wait for it to reopen or go around the bulkhead along the safe route. Players share the goal and can finish alone or together.

## Build and play

With Rokit available, run `rokit install` and `rojo build -o Star-Salvage.rbxl` from this folder. Rokit resolves the version pinned here. Open the resulting local place in Roblox Studio and press Play. Use the displayed proximity prompt: keyboard, touch, and gamepad are supported by Roblox. No asset IDs or external assets are needed.

The station geometry, shared round and per-player HUD are in `src`. Rebuild the place after editing. Use Roblox Studio's device simulator to check the compact HUD and native prompts in portrait and landscape.

| Proof | Input | Expected result |
| --- | --- | --- |
| First pickup | Use the prompt on the amber cell by the airlock | HUD directs the player to the reactor |
| Delivery | Activate the reactor prompt while carrying | A reactor slot lights up and shared count increases |
| Routes | Approach the shutter during each phase | The opening and indicator agree. The route around the bulkhead stays open |
| Win | Deliver all six within 90 seconds | Station saved message, then a fresh round |
| Timeout | Deliver fewer than six and wait 90 seconds | Oxygen depleted message, then a fresh round |
| Multiplayer | Two players each collect and deliver | Both deliveries count toward the same goal |
| Contention | Two clients activate the same battery at once | Only one carries it, and only one delivery counts |
| Departure | One of two players leaves while carrying | Their battery reappears and the other player can deliver it |
| Respawn | Reset a character while carrying a cell | Their cell returns and the HUD guides the player to another pickup |
