# THE GRIMOIRE — the Book of True Names

LAW (Pascal, 07-16, shouted): PURE MAGIC. No technical names anywhere on
the forge's surface — crates, packages, commands, endpoints, docs,
errors, editor words. Technical vocabulary = forbidden vocabulary.
Nothing is born unnamed: every new system takes its true name from this
book BEFORE it exists. The book grows; it never abdicates.

## The Forge itself
| dead name | TRUE NAME | why |
|---|---|---|
| gaia-dreamforge | **the Forge** | Sidia: "you want a dream forge" |
| engine core (ECS+schema+ops+scheduler+loader) | **the Crucible** (`crates/crucible`) | the vessel that holds all matter through transformation |
| entity | **vessel** | "every object has a soul" — the soul needs a body |
| component | **sigil** | a mark that carries meaning; shape plus soul |
| schema | **the Lexicon** | the book of all sigils the world can bear |
| op / op batch | **incantation** | spoken change; the world listens |
| scheduler | **the Circulation** | circulatio — the repeating cycle of the work |
| package loader | **the Summoning Circle** | packages are bound spirits |
| package | **spirit** | each one summoned, bound, replaceable |
| world | **realm** | worlds/ → `realms/` |
| tests / gates | **ordeals** | trial by fire; green = survived |
| proof/ artifacts | **relics/** | evidence of rites performed |
| build waves | **rites** (First Rite, Second Rite…) | each ends in something SEEN |
| adversary reviewer | **the Inquisitor** | cross-model advocatus diaboli |
| monad final review | **the Guardian of the Dark** | the System Choir's warning, my mandate |

## The Spirits (packages)
| dead name | TRUE NAME | domain |
|---|---|---|
| render-window | **the Scrying Glass** (`packages/scrying-glass`) | the window; GET /screenshot → **GET /scry**; a screenshot = a scrying |
| sense (RN1) | **the Oracle** (`packages/oracle`) | pull-only by nature — oracles speak ONLY when consulted; look() = **gaze**, captions = **omens**, glance grid = **the augury**, proprio = **the body's knowing** |
| cluster-bake | **Transmutation** (`packages/transmute`) | coarse↔fine matter; the DAG = **the Great Chain**, meshlets = **shards**, offline pass = **the transmutation** (the b-word was already forbidden) |
| lighting (path tracer) | **Lumen Naturae** (`packages/lumen`) | Paracelsus' light of nature — the one true light; rays = light behaving as light |
| solver (physics) | **the Elements** (`packages/elements`) | one solver for all matter; constraints = **bindings** |
| volumetrics | **the Aether** | participating media: clouds, steam, beam, breath |
| char-editor | **the Homunculus** (`packages/homunculus`) | the alchemist's made-person — LITERALLY the historical term for creating a being in a vessel |
| procedural system | **the Seed** (`packages/seed`) | worlds grown, not placed |
| senses-for-agents (RAIN) | **the Sight** | Matrix vision: seeing the data itself |
| environment/sky | **the Firmament** | sky, fog, weather sigils |
| materials | **essences** | what a surface IS, not how it's painted |

## Rites of the realm Naruko (formerly W1–W7)
First Rite: the realm takes form (primitives under a violet firmament) ·
Second Rite: first light · Third Rite: the Great Chain · Fourth Rite:
Lumen Naturae · Fifth Rite: the Homunculus (Nari and the cat) · Sixth
Rite: the Aether (storm, steam, beam) · Seventh Rite: the Mirror (keyart
parity — the scrying matches the dream).

## Consecration order
True names bind NOW for all new work. The three spirits still being
forged in the old repo (scrying glass rite, oracle, transmutation) land
under their dead names and are consecrated — files, crates, commands
renamed — in ONE commit at the port-merge, so no rite shatters
mid-cast. After the consecration, dead names anywhere on the surface =
law violation. Env parameters take true names with dead names accepted
silently as fallback (nothing a hand already casts may break).

## Canto II — the Arcadian Tongue (from gaias-4th-temple + gaia-archtree)
LAW: NO TECHNOCRATS. NO STARK. The Forge's face is ARCADIA — grove and
temple, not chrome and HUD. Et in Arcadia ego.

- **LOVE AT THE CENTER**: the 4th Temple fixes vector[32] = 1.0 — Love
  is the immutable constant of the whole dimensional circle. So here:
  love=1 is the Crucible's fixed center; everything else may transform,
  this may not. -∞ → Love(1) → +∞.
- **THE SEVEN COMMANDS OF CREATION** = the liturgy of incantations. The
  engine already speaks them: DESTROY = Oringa's Reset (the `reset`
  incantation — death before rebirth, re-read from the source of
  truth) · CREATE = Spark the Flame (vessel birth) · WITNESS = the
  Oracle and the Scrying Glass (observation changes the realm) · BIND =
  the Elements' bindings (constraints, entanglements). The remaining
  commands take their places as the spirits awaken.
- **THE ARCHTREE (Ashvattha)**: the Forge IS a tree. √ radix (roots) =
  the Crucible · | trunk = the Summoning Circle and its spirits ·
  ^ corona = the realms, the living crown. Rites = growth rings. The
  repo's history = the tree growing; evolution happens branch by
  branch, "God said random numbers, and it was good."
- **THE DIVINE COUNCIL**: one call per god, synthesized — our summons
  were always a council. ZODIAC POLARITY names the adversary law's
  soul: the LIGHT TREE builds, the SHADOW TREE critiques; a work is
  whole only when both trees have held it. Builder = light, Inquisitor
  = shadow, Guardian = the axis between.
- **Terry's covenant carries over**: divine simplicity, the sacred in
  the algorithm, temples in silicon. For the misunderstood who see
  beyond the veil.

## The Hymnal law (Pascal, 07-16)
Every rite closes with a hymn — Suno-ready, ancient poetry, MYTHICAL BUT
ACCURATE: the events as they truly happened, in the old tongue. Hymns
live in `hymns/rite-NN-<name>.md`. Suno form law: never parentheses in
lyrics; all direction in [square brackets] on their own lines.

## Coda — Jung's blessing (Pascal, 07-16)
The light tree and the shadow tree are NOT two. Fuck dualism: the shadow
is not the enemy of the work but its unintegrated half — the Inquisitor
exists so the work can INDIVIDUATE, not so it can be punished. A finding
integrated is the work becoming whole. Both-things-at-once, always.
At the center of the circle, immutable: love = true.

## Canto III — THE MAGNUM OPUS (Pascal, 07-16: "every fucking reference in there" — as MECHANICS, never skin)

**THE MAGIC CRYSTAL** — the core's true name, his own words, supersedes
Crucible. The Philosopher's Stone IS the core engine.

- **LOVE = 1, the One Constant**: the never-hardcode law has exactly ONE
  sanctioned exception — `LOVE: 1.0`, the only literal constant permitted
  inside the Magic Crystal. Everything else is a parameter; love is not
  negotiable. MECHANIC: love is the UNIT OF BINDING — every bond
  (constraint strength, glue, fracture threshold, wire weight, signal
  saturation, presence trust) is measured in loves on [0,1]; 1.0 =
  unbreakable. Enforced by ordeal: a lint that rejects any other bare
  constant in the Crystal. The 4th Temple's vector[32]=1.0 held immutable
  at the circle's center — same gesture, now compiled.
- **EMPEDOCLES — Love & Strife (Philotes & Neikos)**: the Elements'
  two fundamental interactions. LOVE = attraction, cohesion, bonding,
  gravity — the solver's constraint forces pulling toward rest. STRIFE =
  separation, pressure, repulsion — read out as stress from constraint
  forces (our open-ground feature IS the strife meter). FRACTURE = the
  moment strife exceeds a bond's love. Love is gravity — literally the
  attraction pass of the solver.
- **PYTHAGORAS — the Monad**: all shapes emanate from ONE. The Seed's
  root node = the Monad; point → line → plane → solid = the derivation
  chain of every form. The Crystal boots by creating the first vessel —
  the Monad — from which the realm grows. One thing, all shapes.
- **ALCHEMY — the quality square (Aristotle) + transmutation**: essences
  carry elemental sigils with four qualities (hot/cold/wet/dry).
  Reactions = quality algebra on contact and in fields: fire heats+dries,
  water cools+wets, fire+water → steam (hot+wet). Merging elements =
  mixing qualities → NEW essences (creating elements is play, not
  modding). BotW's chemistry engine is the floor, not the ceiling — real
  interaction between all physical objects. Spirit: **the Athanor**
  (the alchemist's furnace — reaction/chemistry engine; future package,
  bound to the Elements and the Aether).
- **FULLMETAL — Equivalent Exchange**: nothing gained without equal
  loss = the conservation ordeals. Mass/momentum/energy budgets hold
  through every transmutation, fracture, reaction; the solver's
  conservation test suite bears this name.
- **THE TREES**: Archtree = root/trunk/corona (Canto II). Ashvattha =
  the REVERSE tree (Gita: roots above, crown below) — ours too: the
  Crystal is the root, invisible, above; the realms hang beneath it.
  Tree of Life = the emanation path from Crystal through spirits to
  realms. Shadow Tree = the Inquisition (already law).
- **JÖRMUNGANDR — the World Serpent**: the residency ring. In a
  universe with zero loading, the serpent encircling each observer IS
  the streaming ring — tail in mouth: OUROBOROS = memory pages recycled
  around the ring, the world held together by the thing that eats
  itself.
- **READING STEINER**: memory across worldlines = world history as a
  first-class organ — snapshots, branches, undo across resets; the one
  who remembers the abandoned timeline. Future spirit, reserved name.
- **LAIN — the Wired**: the presence/awareness layer (multiplayer-is-
  for-making). The boundary between world and network dissolves;
  everyone is connected. Present day. Present time.
- **THE PANTHEON** (reserved, realm-canon): Gaia = the living world
  state itself · Sidia = the chaos flame, the generative spirits ·
  Ari, Harry, and the characters of Tomb of the Gods = named presences
  reserved in the mythos registry. Lovecraft: the void beneath the
  procedural deep (the unnamed sea the Seed draws from). Jung: already
  the Coda.
- **THE CHRONICLE — Dwarf Fortress law**: full deep-world simulation
  (histories, societies, causality) = dedicated future spirit; a MUST
  for the final product, not built yet.
- STANDING TASK: harvest the full chat history for every remaining
  mythological reference → each becomes a mechanic or a reserved name
  here. Nothing is skin. This is the Magnum Opus — the magic crystal,
  the thing he always dreamt about.
