# Other player packets

## Evidence and current parser

The 7 October 2026 town capture contains 138 distinct visible player names, according to the capture census supplied with this task. `45 36` is dispatched to `nickname::parse_other` in both live and nickname-only modes. The parser recognizes an actor ID, a sanitized UTF-8 name, a job/class byte where it matches the known job table, a structurally plausible server ID when found, and optional combat power. `src-tauri/tests/fixtures/45-36-town.hex` retains eight short payloads copied at the opcode boundary from the server-to-client flow on port 13328 in `.captures/pets-2026-10-07.pcapng`. The parser tests confirm all eight IDs are nonzero, names decode (including Cyrillic and Han), and all eight job bytes map to the expected classes. The fixture test intentionally does not treat heuristic server-ID or CP scans as proof of a fixed packet layout.

The name parser searches a small range of candidate offsets because preceding metadata varints are not fully understood. It keeps the longest valid sanitized candidate. Server ID and CP use heuristics already in `nickname.rs`; those optional values should be treated as observed only when the parser returns them, not as a documented fixed byte offset. No exact packet schema has been established for the still-unknown fields.

## Decoded fields

| Field | Status | Evidence / behavior |
| --- | --- | --- |
| Actor ID (`id`) | Decoded | Unsigned varint beginning at payload offset 2. The captured fixtures resolve to nonzero IDs. |
| Name (`name`) | Decoded | UTF-8 length-prefixed text selected from candidate offsets and sanitized. The fixtures include `Sempialia`, `Smolsaka`, `Анджелина`, and `辣条哟i`. The 7 October town capture reports 138 distinct names. |
| Server (`server`) | Optional, heuristic | Parser searches after the name/job for a two-byte value accepted by the active region profile. The field is absent when no candidate passes. The server catalog is not a class/race/faction decoder. |
| Class (`class`) | Optional, mapped | A byte after the selected name is mapped through the existing job table (for example job values 5–8 → Gladiator, 9–12 → Templar). Unmapped values stay unknown. |
| Combat power (`cp`) | Optional, marker-based | Parser looks for `F4 CB 1F` and validates a bounded little-endian value following it. Absent if the marker/value pattern is not found. |
| Level | Unknown | No validated level field in the inspected parser/capture samples. |
| Legion/guild | Unknown for `45 36` | No stable legion-name boundary is implemented or confirmed for this packet. |
| Race/faction | Unknown | No confirmed race/faction field. A server ID is not sufficient evidence to assign one. |
| Position | Unknown | No confirmed coordinates are parsed from the visible-player update. |
| Title | Unknown | No confirmed title field. |
| Gear score | Unknown | No confirmed gear-score field. CP is a separate optional value and is not treated as gear score. |
| Despawn | Unknown | No packet is confirmed as a nearby-player despawn. Rows expire after 120 seconds without another `45 36`. |

## Other observed opcodes

- `45 37`: raw occurrences and an actor-like varint are insufficient to prove leave or despawn semantics. A former party-leave interpretation was a guess; the service no longer removes a party/nearby entry from this opcode.
- `41 36`: currently parsed by the existing summon/mob path. Its nearby player/legion meaning has not been established by the inspected code, so it does not update the nearby table.
- `33 8A`: no parser/dispatch handler exists. Legion-like strings in it remain unassigned to players.
- `04 8D`: current handler interprets a constrained layout as summon ownership. Names found in some 30-byte messages are not enough to identify a player metadata schema, and this opcode does not update nearby players.
- `41 36`/`33 8A`/`04 8D` may merit further capture work, but their fields are intentionally not inferred here.

## Service behavior

`GET /v1/nearby` exposes the currently retained visible-player rows. `45 36` refreshes `lastSeen`; rows older than 120 seconds expire and capacity is 500. `inParty` is true only for self until party membership is confirmed independently of the visible-player broadcast. `seenDamage` sums observed damage attributed to that actor while the row is retained. Because coordinates are unknown, `self` coordinates and `lastPosition` are null and the current response falls back to ID order rather than distance order.
