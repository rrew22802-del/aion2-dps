# Party packet notes

## `45 36`

The parser decodes visible-player metadata from `45 36` (ID, name, optional class, server ID and CP); it does not add these actors to the party roster. Town capture analysis shows the opcode for many visible names, so treating it as party-only would be incorrect. See [PACKETS-players.md](PACKETS-players.md) for observed fields and limitations.

## `45 37` and `45 38`

Earlier versions guessed that `45 37` meant party leave and `45 38` meant party disband from a few raw byte contexts. Those contexts did not establish packet boundaries or transaction meaning, and the old tests were synthetic. The parser now ignores `45 37` for party and nearby state. `45 38` is still a provisional party-clear interpretation and needs an isolated capture transaction before it should be considered confirmed.

No separate, confirmed party-only membership opcode or alliance roster has been identified. The current service marks only the local character as `inParty`; other nearby players remain false until membership can be independently established.
