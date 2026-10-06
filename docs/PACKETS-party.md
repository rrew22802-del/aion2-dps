# Party packet notes

Inspected the existing `45 36` parser and `.captures/auto-1791235985495.aetherpc` (the server-to-client game stream on port 8888). The capture is aetherpc TCP payload chunks, so byte contexts below are fragments in the reassembled stream rather than complete, independently decoded messages. The capture does not contain an isolated, protocol-annotated party transaction; opcode/action meanings beyond the existing member-info dispatch are working interpretations.

## `45 36`: member metadata

This opcode is already dispatched in `capture/processor.rs` to `nickname::parse_other`. Its parser reads:

| Field | Encoding / parser behavior |
| --- | --- |
| Opcode | `45 36` |
| Actor ID | unsigned varint starting at payload offset 2 |
| Two leading fields | two unsigned varints; their meaning is not established |
| Name | UTF-8 length-prefixed text; parser scans a small range of candidate offsets and sanitizes the selected name |
| Job/class | one byte after the selected name; existing job-to-class mapping |
| Server ID | heuristically located after the name/job |
| Combat power | optional `F4 CB 1F` marker and the existing bounded value scan |

Examples from the auto recording (the bytes before `45 36` are surrounding stream bytes):

```text
... F0 2A A5 0C 45 36 99 37 07 B0 A0 01 07 09 69 6E 6F 6B 65 6E 74 69 79 06 00 00 00 01 02 ...
... 2C 85 01 96 0C 45 36 AD 5A 03 20 A0 03 07 06 76 61 6C 6F 75 78 0F 00 00 00 02 01 ...
... 2C 85 01 DF 0A 45 36 D0 73 01 20 A0 01 07 08 4F 6C 69 76 65 69 72 61 07 00 00 00 02 01 ...
```

The first fragment includes the UTF-8 name `inokentiy`; the next two include `valoux` and `Oliveira`. For the first entry, bytes immediately after the opcode are `99 37`, the actor-ID varint. These packets are used to add or refresh known member metadata (ID, name, class) in the service roster. Since the capture parser does not label the records as party-only versus nearby-player metadata, this interpretation is limited to the documented member-info opcode and the owner's request; alliance data is not decoded.

## Leave and disband candidates

Raw scans found byte pairs `45 37` (four occurrences) and `45 38` (one occurrence) in this stream. Example contexts:

```text
... F4 35 F4 83 03 45 37 00 00 01 01 1C 18 A4 46 50 A2 ...
... 1E 8E 96 5E 84 45 38 CF 80 AA 37 69 8B 60 17 74 15 ...
```

These contexts alone do not prove packet boundaries or field offsets. The implementation treats `45 37` as a leave event with an actor-ID varint at payload offset 2, and `45 38` as a party-clear/disband event. This is a provisional interpretation: the snippets do not yet validate the `45 37` actor ID, nor establish whether `45 38` may serve another purpose. The parser rejects an invalid/zero leave ID and limits `45 38` handling to that opcode. Member-info refresh updates names/classes in place; a main-character change clears the previous roster. Tests exercise these assumed event layouts with synthetic payloads.

No alliance-specific field or separately identifiable alliance roster was established in these captures. The API therefore exposes the party roster only.
