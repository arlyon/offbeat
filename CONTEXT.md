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
