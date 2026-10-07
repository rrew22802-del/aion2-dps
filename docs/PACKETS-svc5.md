# Evidence and limits for service fight extensions (svc5)

## Confirmed inputs

- Incoming/outgoing combat amounts and exact skill codes use the existing `04 38` parser. The parser already produces target ID, source/actor ID, skill code, amount, and critical flag. Damage taken is recorded only when the target is a known player; it is attached to the latest known encounter target because `04 38` does not identify a dungeon run or encounter on its own.
- Healing amounts use the existing healing skill-code list and decoded amount from `04 38`. A heal is eligible for recap only when its target is a known player. Packets with no decoded amount contribute no heal event.
- Buff recipient, skill code, duration, server timestamp, and actor/applier are decoded by the existing `2A 38` / `2B 38` parser. The fight endpoint reports the recorded intervals and observed applier actor IDs; uptime is the union of clipped intervals.
- The `00 8D` player HP parser reads the entity varint, three intermediate varints, and little-endian HP. A zero HP update for an already-known player creates a death recap from that player's ten-entry recent event deque. The observed capture example is `00 8D 8C 95 04 02 01 00 00 00 00 00 …` from `.captures/auto-1791320045289.aetherpc`; the parser test uses the captured prefix through the complete HP value.
- Boss-cast rows are observations of an existing damage packet whose actor entity has already been identified by the existing NPC parser. They record the NPC catalogue code, exact skill code, and packet time. They do not claim an animation cast start or a cast without a damage packet.

## Bounds and unknown data

- Recap ring: 2,000 player entries × 10 events. Death rows: 128 per retained target. Damage taken: 512 retained targets × at most 24 players × 128 source/skill pairs. Boss damage/cast rows: 512 retained targets × 5,000 entries.
- Buff storage uses the existing 1,024 target cap and 1,024 intervals per target/applier/skill.
- No confirmed dungeon-entry/map identity packet is available in the inspected packet notes and captures. `/v1/history.mapId` is therefore `null` rather than inferred from field-boss timer maps.
- The checked-in `.aetherpc` files are TCP capture chunks, not isolated decrypted application messages. A scan of chunks found opcode byte coincidences but did not establish complete valid `04 38`, `2A 38`, or `2B 38` payloads. No synthetic bytes are presented as capture evidence. Follow-up captured-byte parser fixtures for those packet families require a recording of isolated/decrypted messages from the live dispatcher.
