# Domain Context

## Festival Location

A place where an attendee can declare themselves to be during one festival. A Festival Location is a Stage, the Campsite, or a user-named Custom Location.

## Campsite

A canonical Festival Location that is always available, independent of the festival stage lineup. It is also the subject of Camp Chat, but being at the Campsite and participating in Camp Chat are separate concepts.

## Festival Check-in

An attendee’s explicit declaration of their Festival Location. A Festival Check-in belongs to the attendee and festival and is shared with all groups they have joined for that festival. It is Fresh for four hours, then retained as a Stale last-known location until refreshed or explicitly cleared.

## Group Presence Projection

The group-visible copy of a Festival Check-in. Each projection is private to one group and does not create public festival-wide presence.

## Fresh Check-in

A Festival Check-in made or refreshed within the previous four hours. Fresh check-ins count toward current on-site presence.

## Stale Check-in

A last-known Festival Location whose check-in is more than four hours old. It remains visible at that location with its original timestamp but does not count toward fresh on-site presence.

## Not Shared

The absence of any retained Festival Check-in, either because one has never been made or because it was explicitly cleared.

## Artist Billing

**Source Billing**:
The exact text a schedule source uses to name a performance. It remains the attendee-visible title even when its artist identities are resolved.
_Avoid_: Cleaned title, canonical billing

**Artist Identity**:
A canonical musical artist or act that may appear under multiple names across festivals and Source Billings.
_Avoid_: Billing, set title

**Artist Credit**:
A relationship from a Source Billing to an Artist Identity, including the name and role used in that billing.
_Avoid_: Parsed artist

**Performance Qualifier**:
A description of how a performance is presented, such as DJ set, live set, or ambient set, which does not change Artist Identity.
_Avoid_: Artist suffix

**Presented Title**:
The named event or programme introduced by a presenter within a Source Billing, such as Tea Dance.
_Avoid_: Artist name

**Billing Resolution**:
A reusable, evidenced interpretation of a Source Billing into Artist Credits, a Presented Title, and Performance Qualifiers.
_Avoid_: Enrichment
