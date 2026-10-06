# Combat power and deaths in captured packets

Research performed against `.captures/auto-1791320045289.aetherpc` (7,882 records), `.captures/live-2026-10-06-happ.pcapng` (23,651 parsed frames), and the existing Rust packet dispatch/parsers. The pcapng interface uses raw IPv4 link type 101; all 23,651 frames were TCP traffic for game port 13328. Raw segment scans saw 226 `F4 CB 1F` marker occurrences, 63 `45 36` byte occurrences, and 8,781 `00 8D` occurrences. These are byte occurrences in TCP segments, not counts of decoded application messages. The capture stores TCP payload chunks, not already isolated application packets, so byte examples below are evidence fragments; opcode offsets refer to the decoded payload after stream assembly and prefix removal.

## Combat power

- Party member metadata is dispatched by opcode `45 36` in `capture/processor.rs` to `parser/nickname.rs::parse_other`. Actor ID is the varint beginning at payload offset 2. The parser finds the member name/job/server fields and then searches the same payload for marker `F4 CB 1F`.
- The current CP extractor starts looking at `marker_offset + 11`, then scans byte-by-byte for a little-endian `u32` in `1..=10,000,000` followed by a zero `u32`. The selected value is written to `actor_id_combat_power_map` by `set_actor_combat_power`.
- The main-character update has its own opcode `56 36`; its CP is a little-endian `u32` at payload offset 2 (`parse_main_combat_power`).
- The latest live capture independently confirms that both the member-info opcode and CP marker occur in port-13328 traffic; example marker context from the `.aetherpc` recording is `F4 CB 1F B4 04 00 00 78 21 1D 00 F2 00 30 2A 38 …`.
- Example bytes in the latest recording include an assembled TCP chunk with `45 36` at chunk offset 51 and marker `F4 CB 1F` at offset 792. The marker context is `F4 CB 1F B4 04 00 00 78 21 1D 00 F2 00 30 2A 38 …`. This confirms the member-info/marker signature in the recording; the code's `+11` scan and plausibility check remain heuristic because the recording reader does not expose isolated server messages for a direct field-boundary assertion.

CP is now available as `players[].cp` in `/v1/state` and archived fight state; it is `null` until an update is observed.

## Deaths

There is no separate death opcode in the current opcode table. Death is indicated by a player health update, opcode `00 8D`, where the payload contains the entity ID varint, three intervening varints, and a little-endian `u32` current HP of zero. `parse_remain_hp_packet_at` already decoded the entity and HP, but previously only called `mark_player_dead` while PvP mode was enabled. That guard is removed for death/alive transitions; player death totals are counted independently of PvP mode and are exposed as `players[].deaths`.

Observed zero-HP byte patterns in the latest live capture include `00 8D 8C 95 04 02 01 00 00 00 00 00 …` and `00 8D E7 EB 02 02 01 00 00 00 00 00 …`. The same form occurs in the `.aetherpc` recording. The entity ID is a varint, followed by the three small fields and zero HP. The parser only treats IDs already known as players as death events; mob health updates are excluded.

Death totals are present in current and archived player summaries. The existing PvP endpoint keeps its own kill/assist/death aggregation behavior.

## Limits

The capture snippets establish the opcode families and show that the relevant signatures occur in current traffic. They do not prove every regional/server variant has identical layouts. CP remains guarded by the existing marker/value heuristic. Healing packets are separate: the meter only counts a heal when its decoded packet supplies an amount.
