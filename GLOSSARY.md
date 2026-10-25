# desmack

Words for the assets of a Super Mario Kart (SMK) SNES ROM.

## The ROM

**ROM**:
A dump of the SMK cartridge, with or without a copier header.
_Avoid_: image, file, cartridge

**Region**:
The market a ROM was released for: Japan, USA or Europe.
_Avoid_: type, country, version

**Revision**:
One release of a Region's ROM, e.g. 1.0 or 1.1.
_Avoid_: version, build

**Known ROM**:
A ROM whose checksum matches a catalogued clean dump.

**Modified ROM**:
A ROM that identifies itself as SMK but matches no catalogued dump, e.g. a hack.
_Avoid_: bad dump, invalid ROM

**Copier header**:
512 bytes some dumping devices put in front of the ROM; they carry no game data.

**Asset category**:
One kind of asset desmack extracts on its own, e.g. maps or hazards.

## Courses

**Course**:
One racing circuit, e.g. Mario Circuit 2.
_Avoid_: track, level

**Theme**:
A family of courses sharing one tileset, one genre table and the same scenery, e.g. Mario Circuit.

**Tile**:
An 8×8-pixel square of a course; a course is 128×128 tiles.

**Tileset**:
A theme's 256 tile pictures, numbered 0–255 by tile index. Indices 192–255 are shared by every theme.

**Ground**:
The ground beyond a course's edge, out to the horizon: the theme's tile index 0, repeated.
_Avoid_: floor, outside tiles

**Tile map**:
A course as 128×128 tile indices.

**GP map / TT map**:
The Grand Prix tile map, with the overlay, and the Time Trial tile map, without it.

**Overlay**:
Tiles placed on a course in Grand Prix only: coins, ? panels, oil slicks, zippers, jump bars.

**Genre**:
The surface a tile index stands for, e.g. Road, Grass or Deep water; one table per theme.
_Avoid_: surface, terrain

**Map**:
Everything about a course's layout: its GP and TT tile maps, its theme's genres, its hazard positions, its AI zones and targets, and its start positions.
_Avoid_: level, track data

**Start position**:
Where a driver's kart stands when a race begins.
_Avoid_: grid slot, spawn

**AI target**:
A point on a course that CPU drivers steer towards, with the speed they take it at.
_Avoid_: waypoint, path node

**AI zone**:
An area of a course; a CPU driver inside it steers towards the zone's AI target.
_Avoid_: region, sector

## Things in the race

**Driver**:
One of the eight playable characters: Mario, Luigi, Peach, Yoshi, Bowser, DK Jr., Koopa Troopa, Toad.
_Avoid_: racer, character, kart

**Item**:
Anything a driver collects or throws: the Grand Prix items, the CPU-only items and coins.
_Avoid_: extra, collectable, power-up

**Hazard**:
A moving or standing obstacle drawn as a sprite on the course: pipes, Thwomps, Piranha Plants, Monty Moles, Cheep Cheeps, Bullet Bills.
_Avoid_: obstacle, enemy, track object

**Sky**:
The scenery band above the horizon, made of a near and a far layer.
_Avoid_: background, skybox

**Cast**:
Characters who appear in the game but don't race: Lakitu with his props, and the extras of the ending.
_Avoid_: NPC, extra

**Effect**:
A short-lived sprite drawn by something happening in the race: dust, sparks, splashes, smoke, and the karts' shadows.
_Avoid_: particle, FX

## Around the race

**HUD**:
What is drawn over a race: timer, rank, the player box that holds the item, lives and coins. Laps show on Lakitu's sign, not in the HUD.
_Avoid_: overlay, UI

**Font**:
The game's letters and digits.

**Screen**:
A full picture outside the race, e.g. the title, a select screen or the podium.
_Avoid_: menu, scene
