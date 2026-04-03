/**
 * @fileoverview Game data snapshot contract — type definitions shared between
 * Rust backend and JS frontend.
 *
 * These types mirror the Rust structs in `src/lng/w3e/snapshot.rs` and
 * `src/lng/w3e/slk.rs`.  Both sides MUST stay in sync.
 *
 * ── Data flow ──
 *   1. Rust reads all game files ONCE when the game path is set
 *   2. Builds a `GameSnapshot` (pre-serialised to JSON)
 *   3. JS fetches `/w3e/snapshot` → gets the full snapshot
 *   4. Extension host sends snapshot to webview via postMessage
 *   5. Webview applies data directly (westrings, SLK catalogs, etc.)
 */

// ─── Top-level snapshot ──────────────────────────────────────────────────────

/**
 * The complete game data snapshot returned by `/w3e/snapshot`.
 * @typedef {Object} GameSnapshot
 * @property {Object<string, string>} westrings - WESTRING_* → resolved value map
 * @property {TerrainSlkResult|null} terrainSlk - Terrain tile data
 * @property {DoodadsSlkResult|null} doodadsSlk - Doodad catalog
 * @property {UnitsSlkResult|null} unitsSlk - Unit catalog (merged from SLK + TXT)
 * @property {DestructablesSlkResult|null} destructablesSlk - Destructable catalog
 */

// ─── GameString ──────────────────────────────────────────────────────────────

/**
 * A string value that may have been resolved from a WESTRING_* reference.
 * Plain strings are serialised as-is; resolved values are objects.
 * @typedef {string|{value: string, original: string, source: string}} GameString
 */

// ─── Terrain ─────────────────────────────────────────────────────────────────

/**
 * @typedef {Object} TerrainTileInfo
 * @property {string} tileId - Rawcode, e.g. "Ldrt"
 * @property {string} dir - Texture directory path
 * @property {string} file - Texture file base name
 * @property {string} comment - Human-readable name
 * @property {string} ext - Resolved extension (".tga", ".blp", or "")
 */

/**
 * @typedef {Object} TerrainSlkResult
 * @property {string} source - Where the SLK was found
 * @property {TerrainTileInfo[]} tiles - All tile entries
 */

// ─── Doodads ─────────────────────────────────────────────────────────────────

/**
 * @typedef {Object} Color
 * @property {number} r
 * @property {number} g
 * @property {number} b
 * @property {number} a
 */

/**
 * @typedef {Object} Doodad
 * @property {string} doodId - Rawcode, e.g. "APms"
 * @property {GameString} name
 * @property {string} comment
 * @property {string} category
 * @property {string} tilesets
 * @property {boolean} tilesetSpecific
 * @property {string} file - Model path
 * @property {string} doodClass
 * @property {string} soundLoop
 * @property {number} numVar
 * @property {number} defScale
 * @property {number} minScale
 * @property {number} maxScale
 * @property {boolean} canPlaceRandScale
 * @property {number} selSize
 * @property {boolean} useClickHelper
 * @property {boolean} ignoreModelClick
 * @property {number} maxPitch
 * @property {number} maxRoll
 * @property {number} visRadius
 * @property {boolean} walkable
 * @property {boolean} onCliffs
 * @property {boolean} onWater
 * @property {boolean} floats
 * @property {boolean} shadow
 * @property {boolean} showInFog
 * @property {boolean} animInFog
 * @property {number} fixedRot
 * @property {string} pathTex
 * @property {boolean} showInMm
 * @property {boolean} useMmColor
 * @property {Color} mmColor
 * @property {Color[]} vertColors
 * @property {boolean} inBeta
 * @property {number} version
 */

/**
 * @typedef {Object} DoodadsSlkResult
 * @property {string} source
 * @property {Object<string, Doodad>} doodads - Keyed by rawcode u32
 */

// ─── Units ───────────────────────────────────────────────────────────────────

/**
 * @typedef {Object} SlkSource
 * @property {string} name - SLK file name
 * @property {string} source - Where found
 * @property {number} rows - Row count
 */

/**
 * @typedef {Object} UnitInfo
 * @property {string} unitId - Rawcode, e.g. "Hamg"
 * @property {GameString} name
 * @property {string} comment
 * @property {string} sort
 * @property {string} race
 * @property {string} tilesets
 *
 * @property {string} moveTp
 * @property {number} moveHeight
 * @property {number} moveFloor
 * @property {number} turnRate
 * @property {number} propWin
 * @property {number} formation
 * @property {string} pathTex
 *
 * @property {string} targType
 * @property {number} threat
 * @property {number} points
 * @property {number} death
 * @property {number} deathType
 * @property {boolean} canSleep
 * @property {boolean} canFlee
 * @property {number} cargoSize
 * @property {number} prio
 * @property {string} buffType
 * @property {number} buffRadius
 * @property {boolean} fatLos
 *
 * @property {number} level
 * @property {number} hp
 * @property {number} realHp
 * @property {number} regenHp
 * @property {string} regenType
 * @property {number} mana0
 * @property {number} manaN
 * @property {number} realM
 * @property {number} regenMana
 * @property {number} def
 * @property {string} defType
 * @property {number} defUp
 * @property {number} realDef
 * @property {number} spd
 * @property {number} minSpd
 * @property {number} maxSpd
 * @property {number} sight
 * @property {number} nsight
 * @property {number} bldTm
 * @property {number} repTm
 * @property {number} collision
 * @property {string} primary
 * @property {number} str
 * @property {number} strPlus
 * @property {number} agi
 * @property {number} agiPlus
 * @property {number} int
 * @property {number} intPlus
 * @property {boolean} isBldg
 * @property {string} unitType
 *
 * @property {number} goldCost
 * @property {number} lumberCost
 * @property {number} goldRep
 * @property {number} lumberRep
 * @property {number} fmade
 * @property {number} fused
 * @property {number} bountyDice
 * @property {number} bountySides
 * @property {number} bountyPlus
 * @property {number} stockMax
 * @property {number} stockRegen
 * @property {number} stockStart
 *
 * @property {string} file
 * @property {number} modelScale
 * @property {number} scale
 * @property {number} scaleBull
 * @property {number} occH
 * @property {number} selZ
 * @property {number} red
 * @property {number} green
 * @property {number} blue
 * @property {number} teamColor
 * @property {boolean} customTeamColor
 * @property {string} unitSound
 * @property {string} unitClass
 * @property {string} special
 * @property {string} unitShadow
 * @property {string} buildingShadow
 * @property {boolean} shadowOnWater
 * @property {boolean} selCircOnWater
 * @property {number} maxPitch
 * @property {number} maxRoll
 * @property {number} elevPts
 * @property {number} elevRad
 * @property {number} fogRad
 * @property {string} uberSplat
 * @property {boolean} inEditor
 * @property {boolean} hiddenInEditor
 *
 * @property {number} weapsOn
 * @property {number} acquire
 * @property {string} weapTp1
 * @property {string} weapType1
 * @property {string} atkType1
 * @property {number} cool1
 * @property {number} dmgplus1
 * @property {number} dice1
 * @property {number} sides1
 * @property {number} rangeN1
 * @property {string} targs1
 * @property {boolean} showUi1
 * @property {number} dmgPt1
 * @property {number} backSw1
 * @property {string} splashTargs1
 * @property {number} minRange
 * @property {string} weapTp2
 * @property {string} weapType2
 * @property {string} atkType2
 * @property {number} cool2
 * @property {number} dmgplus2
 * @property {number} dice2
 * @property {number} sides2
 * @property {number} rangeN2
 * @property {string} targs2
 * @property {boolean} showUi2
 * @property {number} dmgPt2
 * @property {number} backSw2
 * @property {string} splashTargs2
 *
 * @property {boolean} inBeta
 * @property {number} version
 *
 * @property {string} [tip]
 * @property {string} [ubertip]
 * @property {string} [hotkey]
 * @property {string} [propernames]
 * @property {string} [revivetip]
 * @property {string} [awakentip]
 * @property {string} [editorSuffix]
 * @property {string} [casterUpgradeName]
 * @property {string} [casterUpgradeTip]
 */

/**
 * @typedef {Object} UnitsSlkResult
 * @property {string} source
 * @property {SlkSource[]} sources - Per-file source info
 * @property {Object<string, UnitInfo>} units - Keyed by rawcode u32
 */

// ─── Destructables ───────────────────────────────────────────────────────────

/**
 * @typedef {Object} Destructable
 * @property {string} destructableId - Rawcode, e.g. "ATtr"
 * @property {GameString} name
 * @property {GameString} editorSuffix
 * @property {GameString} comment
 * @property {string} category
 * @property {string} tilesets
 * @property {boolean} tilesetSpecific
 * @property {string} file
 * @property {boolean} lightweight
 * @property {boolean} fatLos
 * @property {number} texId
 * @property {string} texFile
 * @property {string} doodClass
 * @property {boolean} useClickHelper
 * @property {boolean} onCliffs
 * @property {boolean} onWater
 * @property {boolean} canPlaceDead
 * @property {boolean} walkable
 * @property {number} cliffHeight
 * @property {string} targType
 * @property {string} armor
 * @property {number} numVar
 * @property {number} hp
 * @property {number} occH
 * @property {number} flyH
 * @property {number} fixedRot
 * @property {number} selSize
 * @property {number} minScale
 * @property {number} maxScale
 * @property {boolean} canPlaceRandScale
 * @property {number} maxPitch
 * @property {number} maxRoll
 * @property {number} radius
 * @property {number} fogRadius
 * @property {boolean} fogVis
 * @property {string} pathTex
 * @property {string} pathTexDeath
 * @property {string} deathSnd
 * @property {boolean} shadow
 * @property {Color} color
 * @property {boolean} showInMm
 * @property {boolean} useMmColor
 * @property {Color} mmColor
 * @property {number} buildTime
 * @property {number} repairTime
 * @property {number} goldRep
 * @property {number} lumberRep
 * @property {boolean} inBeta
 * @property {number} version
 * @property {boolean} selectable
 * @property {number} selcircsize
 * @property {string} portraitmodel
 */

/**
 * @typedef {Object} DestructablesSlkResult
 * @property {string} source
 * @property {Object<string, Destructable>} destructables - Keyed by rawcode u32
 */


