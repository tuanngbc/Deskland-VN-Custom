# Phase 2: audit of scene meaning, placement and occlusion

Owner correction: Meadow is a boy flying a kite **on grass**, with clouds above
him. A successful shader build or a changing image cannot establish this.
The rejected 027f437 candidate and the 4cdbe4a evidence are retained. Neither is
owner acceptance of Phase 2. This correction still requires hosted verification
and owner UAT. Phase 1 remains accepted; Phase 3 is gated.

The table records a source/composition audit of all 16 scenes, cross-checked
against the actual 4cdbe4a renderer contact sheet and temporal captures. “Retain”
means no inversion found in that audit, not exhaustive visual certification.

| Theme | Intended physical scene / back-to-front order | Finding and correction |
| --- | --- | --- |
| Meadow | Grass, grounded shadows and boy/kitten, cloud shadow, cloud bodies, airborne dandelions | Confirmed inversion: native appended actors after clouds. Split the original grass/cloud passes around the ground actors. Also fix first-use texture binding that exposed the run atlas as multiple figures. |
| Beach | Sand/sea/foam, kayaks, palm ground shadows, palms in foot-depth order, airborne gulls | Palms appended after birds could hide birds; late palm shadows could darken another canopy. Draw all palm shadows before canopies, then visible gulls. Preserve gull ground shadows. |
| Maldives | Underwater fish/turtles, water/foam/kayaks, grounded palm shadows and palms, gulls | Same canopy/gull correction. Water navigation and palm placement use the actual signed shoreline field, not displayed pixel colour. |
| Lake | Bank/underwater stones, submerged fish, partially transparent cloud reflection, ripples, fishing gear and lifted fish | Reflection is water, not an opaque sky cloud. Retain its shader order. Hide the hooked fish from the submerged list and attach the one lifted drawing to its persistent physical body. Return invisible fish continuously to water instead of leaving it stuck outside water. |
| Sky | Sky/stars/moon/meteor, plane and trail, clouds, nearby birds | Retain original intentional order; birds are nearer than cloud layer. No ground actor appended after it. |
| Ocean | Water/wakes/boats/yachts, gulls, cloud shadows and clouds | Retain original order: boats stay under clouds. Wakes now follow actual simulated velocity. |
| Rainforest | Ground/canopy, click ripples, tropical birds, mist, fireflies, rain | Retain original layer order. Birds and fireflies have separate persistent positions; no sprite ground pass is appended on top. |
| Moc Chau | Ground/fallen petals, blossom canopy, birds, mist and airborne petals | Retain original order. Blossoms are tree canopy, not a walking surface. |
| Tet Sakura | Courtyard/fallen petals, blossom canopy/branches, suspended lanterns/envelopes, swallows and flying petals | Generic native top-row lanterns floated over empty courtyard. Choose stable separated anchors in the actual foliage field, including cord support and icon clearance, and restore both ornament types. |
| Cat Hill | Sky/clouds/hills/grass, scenery/cats/yarn by ground depth, framing canopy, butterflies/leaves | Static scenery previously always behind cats. Merge rocks, cushion, bushes, trees, cats and yarn by foot depth; keep framing foliage in front. |
| Stadium | Pitch/marks, players, ball, goal frames/net, confetti/light | Retain original goal/ball occlusion. Ball and players keep persistent motion; no click teleport. |
| Aquarium | Water/light, sand, rooted plants, bubbles/surface, fishing gear, fish and effects | Generic bottom anchors put roots below the visible bed and fish could overlap sand. Use the shader's exact sand curve, water surface envelope and icon sand field for containment/rooting. |
| Commute | Sky/city/bridge/river, road/stop, bus, icon hills and foreground reeds, flying particles | Retain road/foreground layering. Preserve bus drive/brake/doors/depart lifecycle at reference speed. |
| Summer Porch | Distant garden, house/deck, corgis/props by foot depth, columns/roof, foreground objects, bugs/light | Generic corgi slots put dogs outside the deck. Restore audited legacy deck slots, sizes/coats and pose transitions; sort dogs by ground depth and hit-test in reverse order. |
| Cats | Sky/clouds/hill/ground flowers, trees/cats/yarn by foot depth, butterflies/leaves | Confirmed all-trees-before-all-cats and late-yarn error. CPU-sort stable tree anchors and merge with cats/yarn in the GPU scene. Legacy CPU renderer is not restored. |
| Boxing Ring | Gym/back props, ring/back ropes, depth-ordered boxers, front ropes/posts/equipment, impact effects/HUD | Retain original depth order; actor scale uses reference display rather than panorama size. |

References: `assets/shaders/*.frag`, `prototype/aquarium.frag`,
`prototype/summer-porch.frag`, `prototype/commute.frag`,
`prototype/boxing-ring.frag`, `prototype/meadow.html`,
`prototype/meadow-boy.js`, `prototype/meadow-cat.js`, `src/engine.rs`,
`src/themes/cats/mod.rs`, `src/themes/tet_sakura.rs` and the other scene owners.

Hosted checks include controlled overlapping scenes, not just random frames:
boy visible on grass then hidden by an opaque cloud; gull above a palm; cat
behind/in front of a tree. Probe initial-condition controls are compiled only
into the hosted test executable. Raw renderer frames are exported without image
retouching. Geometry/route tests cover shore detours, disconnected water,
mixed DPI, physical display gaps, scale, anchors and fixed-step motion.

Remaining acceptance limits: native Cats remains a GPU reinterpretation of the
legacy artwork; owner comparison of its full visual parity is still pending.
CI software GL cannot establish owner GPU performance or certify every possible
overlap, user icon layout, time, weather and display arrangement. All unfinished
1.0.3 scope remains tracked; no Phase 2 or master-task completion is claimed.
