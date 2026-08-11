# Houghton 2026 artist-resolution review

_Production snapshot: 2026-08-11T17:08:54.026163+00:00. Read-only review; no overrides were applied._

## Result

Reviewed all **137** terminal failures in the snapshot: **69** `needs_review` and **68** `unresolved`.

| Classification | Count | Recommended action |
|---|---:|---|
| Safe existing-profile override | 12 | Eligible for a signed manual override after approval. |
| Identity likely; canonical profile missing | 88 | Create or discover a trusted canonical profile first; do not override yet. |
| Retry with better evidence | 13 | Improve festival-specific evidence/querying before deciding. |
| Intentional non-artist/program item | 22 | Leave unlinked unless product policy expands artist paths to facilitators/talk participants. |
| Remain unresolved | 2 | Preserve source billing and take no action. |

## Critical adjacent finding: Amit and Aneesh

The failed joint billing should **not** be connected to the two currently resolved solo profiles:

- `Amit` points to MusicBrainz `e2023a61-873e-4485-bca6-8af08b984281`, whose links identify **AMIT / AMITSOUND**.
- `Aneesh` points to MusicBrainz `f633a967-f18f-402a-8e8b-c2ecae474c5d`, whose links identify **Aneesh Poojari**.
- The joint-billing evidence instead identifies the Houghton act as Brilliant Corners founders **Amit and Aneesh Patel**, including [Stamp the Wax](https://www.stampthewax.com/2017/06/14/diggers-directory-amit-aneesh), [Crack Magazine](https://crackmagazine.net/article/profiles/sonic-sanctums-amit-aneesh), and an [interview with both founders](https://timecapsulesound.substack.com/p/interview-with-founders-of-brilliant).

This is a false-positive identity problem in the already-resolved solo records, not merely a missing collaboration link. The safe correction requires new trusted canonical profiles for the two Brilliant Corners founders, replacement overrides for `Amit` and `Aneesh`, and then a two-credit override for `Amit & Aneesh`. Reusing the existing solo IDs would make the data worse.

## Proposed safe overrides

These preserve the exact source billing and use only canonical profiles already present in production. Performance wording remains presentation metadata rather than a separate identity.

| Source billing | Proposed credit | Current status | Review basis |
|---|---|---|---|
| Craig Richards (Electro Set) | Craig Richards → Craig Richards (`mbid:85873c02-8093-4a5a-91bc-513722b3fbc0`), performer | `needs_review` | Craig Richards has an existing canonical profile and an exact standalone lineup billing; 'Electro Set' is a performance qualifier, not a separate identity. |
| Craig Richards (Reggae Set) | Craig Richards → Craig Richards (`mbid:85873c02-8093-4a5a-91bc-513722b3fbc0`), performer | `needs_review` | Reggae Set is a performance qualifier, not a separate identity; the performer is the existing Craig Richards profile while the exact source billing remains preserved. |
| Danny Daze ::BLUE:: | Danny Daze → Danny Daze (`mbid:734968e2-c3eb-4d98-a16e-3fc8f652942a`), performer | `needs_review` | Danny Daze is an exact source span and an existing canonical profile. '::BLUE::' is attached presentation/release styling, not a second identity. |
| Greg Paulus Live (Live) | Greg Paulus → Greg Paulus (`mbid:b9cd0256-2361-47a4-b53b-c2c1d8630d80`), performer | `unresolved` | Greg Paulus is an exact source span and existing canonical profile. Both occurrences of 'Live' are performance wording, not distinct identities. |
| Lutz | Lutz → Nicolas Lutz (`mbid:e955e6bc-0ed9-4180-86ac-250894ca130f`), performer | `needs_review` | The supplied canonical Nicolas Lutz profile shares the exact Resident Advisor URL returned for the billing, and 'Nicolas Lutz' also appears as a standalone lineup billing. This sufficiently corroborates the shortened credit. |
| Om Unit pres. Acid Dub Studies (Live) | Om Unit → Om Unit (`mbid:9be5414e-b906-4659-9c59-012ba7f7a154`), performer | `unresolved` | Om Unit has an existing canonical profile and exact standalone lineup context; 'presents Acid Dub Studies (Live)' describes the performance programme, not another identity. |
| Paramida (Balearic Set) | Paramida → Paramida (`mbid:8df5db13-af38-47f8-8c3b-651d4899698c`), performer | `needs_review` | Paramida exactly matches an existing canonical profile. '(Balearic Set)' describes the performance format and must not be treated as another identity. |
| Peter Adjaye (Live) | Peter Adjaye → Peter Adjaye (`mbid:c460d5bb-5add-45e6-af0e-4779ca8a00f2`), performer | `unresolved` | Live is a performance qualifier, not a separate identity, and the existing Peter Adjaye canonical profile safely represents the performer while preserving the full source billing. |
| Radioactive Man (Live) | Radioactive Man → Radioactive Man (`mbid:28b39ed0-5792-4f96-9230-40555528f32c`), performer | `needs_review` | Radioactive Man is an exact source span and canonical name; '(Live)' is solely a performance qualifier. |
| Radioactive Man (Reggae Set) | Radioactive Man → Radioactive Man (`mbid:28b39ed0-5792-4f96-9230-40555528f32c`), performer | `unresolved` | Radioactive Man has an existing canonical profile and exact standalone lineup billings; 'Reggae Set' is a performance qualifier rather than another identity. |
| Ste Roberts (Live) | Ste Roberts → Ste Roberts (`mbid:7334ce47-d06d-4c41-a2f0-615c06dc83f2`), performer | `unresolved` | Ste Roberts exactly matches an existing canonical profile, and '(Live)' is only a performance qualifier. |
| Z@P (Live) | Z@P → Z@p (`mbid:a1110b58-d013-4d62-8776-75bd0b6e29a2`), performer | `unresolved` | Live is only a performance qualifier; the exact standalone Z@P lineup billing, canonical alias/profile data, and artist evidence all identify the existing Z@p profile. |

## Review policy

- Exact Clashfinder source billing remains unchanged.
- No collaboration was split from punctuation alone.
- A safe override requires every proposed credit to reference an existing canonical profile.
- Qualifiers such as `(Live)`, `Reggae Set`, `Balearic Set`, and `Electro Set` are not separate identities.
- Generic or collision-prone names remain blocked without festival-specific corroboration.
- Talks, workshops, wellness sessions, placeholders, and memorial/program titles remain unlinked by default.
- `likely_identity_missing_profile` means evidence probably identifies the act, but the current override API cannot create a canonical profile.

## Complete review

| Source billing | Current | Classification | Confidence | Rationale |
|---|---|---|---:|---|
| .VRIL (Live) | `needs_review` | Identity likely; canonical profile missing | 0.95 | The parenthetical is a performance qualifier, and .VRIL is a well-corroborated artist identity, but no matching canonical profile exists. |

"no matching canonical profile" https://ra.co/dj/vril this has the proper fully stylized name

| 5ive | `needs_review` | Retry with better evidence | 0.93 | 5ive is a generic, collision-prone name and no matching canonical profile exists. The current evidence does not safely identify which act the billing denotes. |
| Active Systems (Live) | `unresolved` | Retry with better evidence | 0.96 | Active Systems may be an artist project, but the current evidence is too noisy to establish the exact identity, and no canonical profile matches. |
| Alex Downey | `needs_review` | Identity likely; canonical profile missing | 0.98 | The billing is an exact artist name repeated across three lineup sets, but no canonical profile exists to credit. |

also on RA.co

| Amit & Aneesh | `unresolved` | Identity likely; canonical profile missing | 0.88 | The billing is well supported as Brilliant Corners founders Amit and Aneesh Patel, but the existing same-name canonical profiles point to other or uncorroborated identities and must not be reused. |

it is probably quite likely that these are the same people and the system must be able to ID them.

| Anna Wall | `needs_review` | Identity likely; canonical profile missing | 0.98 | The exact standalone billing identifies a DJ, but no canonical profile for Anna Wall is available. |

also on RA

| Aoki Takamasa | `needs_review` | Identity likely; canonical profile missing | 0.97 | This is a clearly established artist billing with no corresponding canonical profile in the supplied set. |

https://musicbrainz.org/artist/ce30feb8-6664-4262-8bf3-4e30f4730fc9
https://ra.co/dj/aokitakamasa

| Apparently open but I doubt it | `unresolved` | Intentional non-artist/program item | 0.97 | The exact lineup text reads as an open-slot/commentary placeholder rather than a performer identity and must not be split or forced onto an artist. |

yes, this is legit

| Ario | `needs_review` | Identity likely; canonical profile missing | 0.82 | The billing appears to denote the DJ Ario, but there is no Ario canonical profile among the supplied profiles, so no existing artist ID can be proposed. |

| Axis 89 (Live) | `unresolved` | Retry with better evidence | 0.93 | Live is only a qualifier, but the current packet does not identify the specific Axis 89 act and the name is too ambiguous for an override. |
| Baby Vulture & E/Tape | `needs_review` | Identity likely; canonical profile missing | 0.86 | The source is a collaboration of two supported artist identities, but neither component has an existing canonical profile and neither appears as an exact standalone billing, so no split is safe. |
| Belle Bête | `unresolved` | Identity likely; canonical profile missing | 0.91 | Belle Bête is corroborated as a DJ identity and also appears in an exact lineup collaboration, but no matching canonical profile exists. |
| Bobby. | `needs_review` | Identity likely; canonical profile missing | 0.94 | The punctuation-bearing name appears exactly in two lineup sets and is corroborated as a DJ identity, but no matching canonical profile exists. |

https://ra.co/dj/bobby

| Breathwork Breathe to Restore | `unresolved` | Intentional non-artist/program item | 0.99 | This is a descriptive breathwork session title rather than a performer identity. |
| Brruno Schmidt | `unresolved` | Identity likely; canonical profile missing | 0.91 | The unusual doubled-r source spelling is very likely the lineup's credited form for DJ Bruno Schmidt, but no canonical profile exists and the source spelling must be preserved. |
| Bruno Schmidt | `needs_review` | Identity likely; canonical profile missing | 0.96 | The exact name is consistently documented as an electronic-music artist, but the supplied canonical profiles contain no match. |

https://ra.co/dj/brunoschmidt trivial spelling must be resolved

| C.A.R (Live) | `needs_review` | Identity likely; canonical profile missing | 0.91 | C.A.R is well supported as the performing identity and '(Live)' is only a performance qualifier. No supplied canonical profile represents C.A.R. |

https://ra.co/dj/car-uk

| Cameron Cullen | `needs_review` | Identity likely; canonical profile missing | 0.90 | The exact personal-name billing appears twice and is supported as a music identity, but there is no existing canonical profile. |

https://ra.co/dj/djposture

| CCL | `unresolved` | Identity likely; canonical profile missing | 0.94 | The acronym is consistently corroborated as the electronic artist CCL, but no matching canonical profile is supplied. |
| Cedric Woo | `needs_review` | Identity likely; canonical profile missing | 0.98 | Cedric Woo is a well-corroborated standalone DJ billing with no available canonical profile. |
| Cedric Woo & Belle Bête | `needs_review` | Identity likely; canonical profile missing | 0.84 | Both names also occur as exact standalone lineup billings, supporting a genuine collaboration, but neither has an existing canonical profile; no split override is possible. |
| Chris Sullivan | `unresolved` | Identity likely; canonical profile missing | 0.88 | Although the name is common, the official DJ site, established press, social account, and Houghton listings jointly support the London DJ Chris Sullivan. No canonical profile is supplied. |
| Claude | `needs_review` | Retry with better evidence | 0.97 | Claude is a common name, and the stored results silently expand it to Claude VonStroke without lineup-specific corroboration. That identity leap is unsafe. |
| Claude & Krishan | `unresolved` | Retry with better evidence | 0.36 | The common component names cannot be split safely: Krishan has a possible canonical alias and standalone billing, but Claude lacks a canonical profile and the evidence does not corroborate both exact identities together. |
| Craig Richards (Electro Set) | `needs_review` | Safe existing-profile override | 0.99 | Craig Richards has an existing canonical profile and an exact standalone lineup billing; 'Electro Set' is a performance qualifier, not a separate identity. |
| Craig Richards (Reggae Set) | `needs_review` | Safe existing-profile override | 0.99 | Reggae Set is a performance qualifier, not a separate identity; the performer is the existing Craig Richards profile while the exact source billing remains preserved. |
| Dandy Jack (Live) | `needs_review` | Identity likely; canonical profile missing | 0.96 | Live is only a performance qualifier, and Dandy Jack is well established as Martin Schopf's artist identity, but no supplied canonical profile represents it. |
| Danny Daze ::BLUE:: | `needs_review` | Safe existing-profile override | 0.99 | Danny Daze is an exact source span and an existing canonical profile. '::BLUE::' is attached presentation/release styling, not a second identity. |
| Dave Harvey | `unresolved` | Identity likely; canonical profile missing | 0.94 | Two exact lineup appearances and multiple specific sources identify Bristol DJ/promoter Dave Harvey, but no matching canonical profile exists. |
| Deadbeat (DJ Set) | `unresolved` | Identity likely; canonical profile missing | 0.94 | DJ Set is a performance qualifier, and the lineup's separate Deadbeat (Live) billing plus identity sources point to Scott Monteith's established act; no canonical profile matches. |
| Deadbeat (Live) | `unresolved` | Identity likely; canonical profile missing | 0.95 | The billing is strongly attributable to Deadbeat, the Scott Monteith project, and '(Live)' is a qualifier. No supplied canonical profile represents Deadbeat. |
| Del Glizzy | `unresolved` | Retry with better evidence | 0.24 | The distinctive billing may denote an artist, but the stored evidence does not identify that exact act and no canonical profile exists. |
| DJ Masda | `needs_review` | Identity likely; canonical profile missing | 0.99 | This is an exact artist billing repeated in the lineup, but no canonical DJ Masda profile is available. |
| Dominic Capello | `unresolved` | Identity likely; canonical profile missing | 0.90 | The exact lineup spelling likely refers to Glasgow DJ Domenic Cappello, whose less common name is strongly corroborated, but no canonical profile is present and the spelling discrepancy precludes an override. |
| Double Agent 7 | `needs_review` | Identity likely; canonical profile missing | 0.96 | Double Agent 7 is a distinct, corroborated act name, but there is no matching canonical profile. |
| Dr Horn | `needs_review` | Identity likely; canonical profile missing | 0.83 | The lineup and exact-name music profiles support an artist identity, but the relatively generic name has no supplied canonical match. |
| E/Tape & Baby Vulture | `needs_review` | Identity likely; canonical profile missing | 0.94 | The collaboration is corroborated, but neither E/Tape nor Baby Vulture has a supplied canonical profile. Splitting without IDs for every component would violate the collaboration safety rule. |
| Electro Elvis | `needs_review` | Identity likely; canonical profile missing | 0.94 | Electro Elvis is well supported as Nathan Hernando's performance alias, but neither that identity nor the alias has an existing canonical profile. |
| Elijah Minnelli (Live) | `unresolved` | Identity likely; canonical profile missing | 0.98 | Live is a qualifier and Elijah Minnelli is extensively corroborated as an artist, but the canonical profile set has no match. |
| Ellie Stokes | `needs_review` | Identity likely; canonical profile missing | 0.98 | The exact name occurs in two lineup sets and clearly identifies a DJ, but no canonical Ellie Stokes profile is present. |
| Fabien | `unresolved` | Retry with better evidence | 0.99 | Fabien is a common given name, the stored results point to several different people, and there is no festival-specific identity corroboration or canonical profile. |
| Facta & K Lone | `needs_review` | Identity likely; canonical profile missing | 0.90 | The lineup also contains the exact collaboration with K-Lone's hyphenated styling, and external sources corroborate the joint act; no canonical profile exists for the billing or its components. |
| Facta & K-Lone | `needs_review` | Identity likely; canonical profile missing | 0.97 | Facta and K-Lone are consistently documented as the two collaborators, but neither has an existing supplied canonical ID. |
| Flow and Gong with Lucia Jiménez & Julim | `unresolved` | Intentional non-artist/program item | 0.97 | This is a named wellness session with facilitators, not a single artist identity, and punctuation alone must not trigger component credits. |
| Frank Haag (Live) | `unresolved` | Identity likely; canonical profile missing | 0.98 | Live is a performance qualifier and the underlying Frank Haag artist identity is well supported, but no canonical profile is supplied. |
| Gabriel Rai | `needs_review` | Identity likely; canonical profile missing | 0.97 | The exact artist name appears in two lineup sets and has no canonical profile. |
| Gideön (Reggae Set) | `needs_review` | Identity likely; canonical profile missing | 0.97 | Reggae Set is only a qualifier and the billing refers to UK DJ GIDEÖN, but the supplied Gideon canonical profile is a different act, so it must not be overridden onto this billing. |
| Grace Sands | `unresolved` | Identity likely; canonical profile missing | 0.91 | The ordinary personal name is corroborated by several DJ-specific sources and exact festival context, but no matching canonical profile exists. |
| Greg Paulus Live (Live) | `unresolved` | Safe existing-profile override | 0.99 | Greg Paulus is an exact source span and existing canonical profile. Both occurrences of 'Live' are performance wording, not distinct identities. |
| H Foundation (Hipp-E & Halo) | `unresolved` | Identity likely; canonical profile missing | 0.96 | H Foundation is strongly established as the joint act of Hipp-E and Halo, but no canonical profile exists for the act or both components, so the parenthetical cannot be split into credits. |
| Hackney Electronica (Live) | `needs_review` | Identity likely; canonical profile missing | 0.86 | Live is a qualifier and Hackney Electronica is corroborated as an act, but there is no corresponding canonical profile. |
| Hamish & Toby | `needs_review` | Identity likely; canonical profile missing | 0.95 | Hamish & Toby is a clearly established DJ act, but no supplied canonical profile represents the act or both components. |
| Harri Pepper | `needs_review` | Identity likely; canonical profile missing | 0.98 | The exact billing recurs in three lineup sets and clearly denotes a DJ, but no canonical profile exists. |
| Harry & Dan Present Tea Dance | `unresolved` | Retry with better evidence | 0.38 | The billing is a presented event title, and the generic presenter names Harry and Dan cannot safely be treated as artist components without exact contextual corroboration. |

This was in our test set also???

| Harry McCanna | `needs_review` | Identity likely; canonical profile missing | 0.99 | Harry McCanna appears exactly in three lineup sets and is strongly established as a DJ, but no canonical profile is supplied. |

https://ra.co/dj/harrymccanna

| Holy Tongue feat. Dali De | `needs_review` | Identity likely; canonical profile missing | 0.88 | The source is a genuine featured-artist billing, but no canonical profile exists for either credited identity and the supplied standalone-lineup condition for a component override is absent. |
| Howie B & Hiraki Sawa (Live) | `needs_review` | Identity likely; canonical profile missing | 0.90 | Howie B and Hiraki Sawa are corroborated collaborators and '(Live)' is a qualifier, but neither component has a supplied canonical artist ID. |
| Itchy Rich | `needs_review` | Identity likely; canonical profile missing | 0.91 | The billing is supported as Rich Normile's artist alias, but there is no corresponding canonical profile. |

https://ra.co/dj/itchyrich

| Jane Fitz & Paquita Gordon | `needs_review` | Identity likely; canonical profile missing | 0.98 | The collaboration and both components are corroborated, but only Jane Fitz has an existing canonical profile; strict collaboration safety prohibits a partial override while Paquita Gordon's profile is missing. |
| Jason Lindner & Currency Audio | `unresolved` | Identity likely; canonical profile missing | 0.87 | An exact prior event and performance video corroborate the collaboration, and Jason Lindner also appears in standalone lineup context, but neither component has a supplied canonical profile. |
| Jason Lindner (Live Keyboard Sound) | `unresolved` | Identity likely; canonical profile missing | 0.96 | Jason Lindner is a clearly corroborated keyboard performer; the parenthetical describes the performance and is not an identity. No supplied canonical profile matches him. |
| Jonas Robinson | `needs_review` | Retry with better evidence | 0.78 | The common full name needs source-specific identity corroboration before concluding that the Houghton billing is the RA artist Jonas 'Stackhouse' Robinson. |
| Josh Caffé (Live) | `unresolved` | Identity likely; canonical profile missing | 0.99 | Josh Caffé is clearly a music artist and 'Live' is a performance qualifier, but no canonical profile exists. |
| Kian Ok | `needs_review` | Identity likely; canonical profile missing | 0.94 | Kian Ok is consistently identified as a DJ across exact-name profiles, but no matching canonical profile exists in the supplied set. |
| Kyle Toole | `needs_review` | Identity likely; canonical profile missing | 0.96 | The exact artist billing appears repeatedly, including in another collaboration context, but lacks a canonical profile. |
| Kyle Toole & Kian Ok | `needs_review` | Identity likely; canonical profile missing | 0.95 | The joint billing and both named performers are supported, but neither component has an existing canonical profile. |
| Life Drawing with Sophia Shuvalova | `unresolved` | Intentional non-artist/program item | 0.99 | This is explicitly a life-drawing workshop/session led by Sophia Shuvalova, not a music-artist billing. |
| Livwutang | `needs_review` | Identity likely; canonical profile missing | 0.95 | Livwutang is a distinctive, corroborated artist alias, but no supplied canonical profile matches the alias or identified person. |
| Loula Yorke (Live) | `unresolved` | Identity likely; canonical profile missing | 0.98 | Live is a qualifier and Loula Yorke is clearly established as a live modular-synthesis artist, but no canonical profile matches. |
| Lutz | `needs_review` | Safe existing-profile override | 0.90 | The supplied canonical Nicolas Lutz profile shares the exact Resident Advisor URL returned for the billing, and 'Nicolas Lutz' also appears as a standalone lineup billing. This sufficiently corroborates the shortened credit. |

Yes obviously!!

| Marie Davidson (Live) | `unresolved` | Identity likely; canonical profile missing | 0.96 | Marie Davidson is strongly identified as the artist, with '(Live)' only indicating performance format. No supplied canonical profile exists for her. |
| Mathew Jonson & .VRIL (Live) | `needs_review` | Identity likely; canonical profile missing | 0.93 | Mathew Jonson has a corroborated canonical profile and .VRIL has exact standalone lineup context, but .VRIL lacks a canonical profile; strict collaboration safety therefore bars a partial or split override. |
| Maybe Laura | `needs_review` | Identity likely; canonical profile missing | 0.96 | Maybe Laura is a distinctive and consistently corroborated DJ identity with no matching canonical profile. |
| Melchior Productions LTD (Live) | `needs_review` | Identity likely; canonical profile missing | 0.95 | Live is a qualifier and Melchior Productions Ltd is a documented Thomas Melchior project, but no matching canonical artist ID is supplied. |
| Melina Serser | `unresolved` | Identity likely; canonical profile missing | 0.99 | The exact artist identity is comprehensively corroborated but absent from canonical profiles. |
| Midland | `unresolved` | Identity likely; canonical profile missing | 0.94 | In this electronic-festival context, the exact billing is well corroborated as DJ/producer Midland, but no matching canonical profile is present. |

??? https://musicbrainz.org/artist/35fddf9e-01f6-49af-8d03-b841dda89995

| Midland (Ambient Set) | `unresolved` | Identity likely; canonical profile missing | 0.91 | Ambient Set is a performance qualifier, and standalone lineup context plus specialist sources identify the electronic artist Midland; the supplied profiles lack that identity. |

We talked about this! It's in the fucking test set???

| Miles Wonderfulsound | `unresolved` | Identity likely; canonical profile missing | 0.91 | The distinctive Wonderfulsound context consistently identifies this billing as DJ Miles Copeland, but no matching canonical profile is supplied. |
| Morning Flow with Andy Kobelinsky | `unresolved` | Intentional non-artist/program item | 0.98 | This is a morning yoga/flow session facilitated by Andy Kobelinsky rather than an artist billing. |
| Moy (Live) | `needs_review` | Identity likely; canonical profile missing | 0.80 | The billing likely denotes MOY (UK), with '(Live)' functioning only as a qualifier, but no matching canonical profile is supplied. |
| Nik Bartsch's Ronin (Live) | `unresolved` | Identity likely; canonical profile missing | 0.99 | Live is a qualifier on the established group identity Nik Bärtsch's Ronin; no canonical profile for the group is supplied. |
| Nik Bärtsch (Solo Piano) | `unresolved` | Identity likely; canonical profile missing | 0.99 | Solo Piano describes the performance format rather than a separate identity, and no canonical profile exists for Nik Bärtsch. |
| NULLPTR (Live) | `needs_review` | Identity likely; canonical profile missing | 0.90 | NULLPTR is supported as Eddie Symons' live artist alias, but no existing canonical profile can receive the credit; 'Live' is only a performance qualifier. |
| O.BEE & Tomas | `unresolved` | Identity likely; canonical profile missing | 0.95 | Evidence and exact lineup context support O.BEE with Tomas Station, but only O.Bee has a supplied canonical profile. A partial collaboration override is unsafe because Tomas/Tomas Station lacks an existing canonical ID. |
| O.Bee & Tomas Station | `needs_review` | Identity likely; canonical profile missing | 0.98 | The collaboration is well supported, but only O.Bee has an existing canonical profile; Tomas Station is missing, so a partial collaboration override is unsafe. |
| Oli Silva | `needs_review` | Identity likely; canonical profile missing | 0.93 | Multiple exact-name music profiles corroborate the Houghton artist, but the canonical profile set has no match. |

https://ra.co/dj/olisilva

| Om Unit pres. Acid Dub Studies (Live) | `unresolved` | Safe existing-profile override | 0.99 | Om Unit has an existing canonical profile and exact standalone lineup context; 'presents Acid Dub Studies (Live)' describes the performance programme, not another identity. |
| Paquita Gordon | `unresolved` | Identity likely; canonical profile missing | 0.98 | Paquita Gordon is strongly corroborated as a DJ and appears as an exact standalone lineup billing, but no canonical profile is present. |
| Paramida (Balearic Set) | `needs_review` | Safe existing-profile override | 0.99 | Paramida exactly matches an existing canonical profile. '(Balearic Set)' describes the performance format and must not be treated as another identity. |
| Pariah | `unresolved` | Identity likely; canonical profile missing | 0.95 | Despite the generic word, multiple aligned UK electronic-music sources identify Pariah as Arthur Cayzer; no canonical profile matches. |
| Patrick Rowe | `unresolved` | Identity likely; canonical profile missing | 0.92 | The full-name billing is corroborated as a DJ identity in the relevant scene and Houghton context, but no matching canonical profile is supplied. |
| Peach | `needs_review` | Identity likely; canonical profile missing | 0.88 | The generic billing is sufficiently corroborated as DJ Peach/Serena Pasion in this festival context, but no canonical profile exists. |
| Peach (R&B Set) | `unresolved` | Identity likely; canonical profile missing | 0.84 | The exact standalone Peach billing and separately corroborated DJ Peach identity make 'R&B Set' a performance qualifier, but no canonical Peach profile exists. |
| Peter Adjaye (Live) | `unresolved` | Safe existing-profile override | 0.99 | Live is a performance qualifier, not a separate identity, and the existing Peter Adjaye canonical profile safely represents the performer while preserving the full source billing. |
| Peverelist (Live) | `unresolved` | Identity likely; canonical profile missing | 0.97 | Live is a qualifier and Peverelist is a distinctive, well-corroborated artist identity absent from the canonical profiles. |
| Powder | `needs_review` | Identity likely; canonical profile missing | 0.92 | Although Powder is a common word, multiple scene-specific sources corroborate the Japanese DJ/producer identity associated with Moko Shibata; no canonical profile is present. |
| Prince Fatty, Horseman, Liam Bailey, Ignition High Power and Mr Williamz | `needs_review` | Retry with better evidence | 0.94 | The five-part billing cannot be safely split because there are no exact standalone lineup billings and the stored evidence does not corroborate every component. |
| Quest | `unresolved` | Retry with better evidence | 0.98 | Quest is a generic, collision-prone name and the stored evidence mixes multiple plausible DJs without tying one to the Houghton billing. |
| Radioactive Man (Live) | `needs_review` | Safe existing-profile override | 0.99 | Radioactive Man is an exact source span and canonical name; '(Live)' is solely a performance qualifier. |
| Radioactive Man (Reggae Set) | `unresolved` | Safe existing-profile override | 0.99 | Radioactive Man has an existing canonical profile and exact standalone lineup billings; 'Reggae Set' is a performance qualifier rather than another identity. |
| Reggie Watts (Live) | `unresolved` | Identity likely; canonical profile missing | 0.99 | Live is only a qualifier and the billing clearly refers to performer Reggie Watts, but no canonical profile is supplied. |
| Remi Mazet (Live) | `unresolved` | Identity likely; canonical profile missing | 0.97 | Live is a qualifier, and Remi Mazet is directly corroborated as an artist and as a prior Houghton live performer; no canonical match exists. |
| Renata | `needs_review` | Retry with better evidence | 0.37 | Renata is too common a name to connect safely to Renata Sabella without stronger festival-specific or cross-source corroboration. |
| Robert Mitchell (Live Solo Piano) | `needs_review` | Identity likely; canonical profile missing | 0.95 | Live Solo Piano is a qualifier that disambiguates the common name toward jazz pianist Robert Mitchell, but no matching canonical profile exists. |
| Saoirse | `needs_review` | Identity likely; canonical profile missing | 0.95 | The common given name is sufficiently corroborated in a consistent electronic-music context as Saoirse Ryan's artist identity, but no canonical profile exists. |

https://musicbrainz.org/artist/4dcb5fd1-53b9-4c2c-a3f8-53c11647daa4 ???

| Scientist (Live) | `needs_review` | Identity likely; canonical profile missing | 0.90 | The evidence consistently identifies dub musician Scientist (Hopeton Brown), and '(Live)' is a qualifier, but Scientist has no supplied canonical profile. |

https://musicbrainz.org/artist/90d63976-5e70-4c73-a500-eb67997d6ed2 ???

| Scott Pelloux | `needs_review` | Identity likely; canonical profile missing | 0.96 | The exact, distinctive personal-name billing is well corroborated but has no canonical profile. |
| Sha & James Gilligan (Live) | `unresolved` | Identity likely; canonical profile missing | 0.82 | The exact paired live billing is independently documented, but neither Sha nor the relevant James Gilligan has a supplied canonical profile. No punctuation-only split should be proposed. |
| Shay Malt | `needs_review` | Identity likely; canonical profile missing | 0.94 | Shay Malt is supported as a performer identity, but no canonical profile is available. |
| Soundbath | `unresolved` | Intentional non-artist/program item | 0.99 | Soundbath is a generic wellness activity title, not a safely identifiable artist name. |
| Soundbath with Michelle Cade | `unresolved` | Intentional non-artist/program item | 0.98 | The exact billing describes a soundbath/wellbeing session facilitated by Michelle Cade rather than a standalone artist act. |
| Soundbath with Veronika | `unresolved` | Intentional non-artist/program item | 0.97 | This is a facilitated soundbath/wellness activity, not an artist performance billing; the lineup also lists Soundbath and another named-facilitator variant. |
| Ste Roberts (Live) | `unresolved` | Safe existing-profile override | 0.99 | Ste Roberts exactly matches an existing canonical profile, and '(Live)' is only a performance qualifier. |
| Studio Batsumi | `unresolved` | Identity likely; canonical profile missing | 0.98 | Studio Batsumi is a well-corroborated exact artist identity appearing in two lineup sets, but no canonical profile exists. |
| Sugar Free | `needs_review` | Identity likely; canonical profile missing | 0.91 | Although the name is a generic phrase, the evidence consistently identifies the Madrid-born, Berlin-based DJ; the canonical profile set lacks that artist. |

https://ra.co/dj/sugarfree

| Sven Figee (Solo Piano) | `needs_review` | Identity likely; canonical profile missing | 0.96 | Sven Figee is an unambiguous full-name performer and '(Solo Piano)' describes the performance. No matching canonical profile is supplied. |
| Sven Hammond (Live) | `needs_review` | Identity likely; canonical profile missing | 0.86 | Sven Hammond is supported as a live music act and 'Live' is a qualifier, but no canonical profile exists. |
| Talk: Balance Presents with Shanti Celeste, Luke Una and Tom Colville | `unresolved` | Intentional non-artist/program item | 0.99 | The exact source explicitly labels a Balance-presented talk with named participants, not a collaborative artist performance. |
| Talk: Digging Deep: Trevinos x Inverted Audio with Sonja Moonear, Dr Banana & Tristan Da Cunha | `unresolved` | Intentional non-artist/program item | 0.99 | The exact source explicitly denotes a titled talk, not a music-performance artist billing, so its named participants should not be converted into performer credits. |

YES BUT YOU HAVE THE PEOPLE WHO ARE PARTICIPATING???

| Talk: RA 'Playing Favourites' with Jane Fitz | `needs_review` | Intentional non-artist/program item | 0.99 | The exact billing explicitly denotes a Resident Advisor talk featuring Jane Fitz, not a standalone music-artist billing to resolve. |

YOU HAVE THE ARTIST??? WHY???

| TBC | `unresolved` | Intentional non-artist/program item | 0.99 | In a festival timetable, TBC is the standard 'to be confirmed' placeholder, not an artist identity. Search matches for performers named DJ TBC must not override that context. |
| Terminus C | `unresolved` | Intentional non-artist/program item | 0.98 | This is one member of a systematic Terminus letter/symbol placeholder series in the lineup, not a corroborated artist identity. |
| Terminus D | `unresolved` | Intentional non-artist/program item | 0.97 | Within the supplied lineup, Terminus D is one of a systematic series of Terminus letter/symbol labels, indicating a location or program marker rather than an artist identity. |
| Terminus F | `unresolved` | Remain unresolved | 0.97 | Terminus F appears amid a systematic series of Terminus letter/symbol billings, but the packet does not establish whether these are artist identities, program slots, or installation labels. |
| Terminus M | `unresolved` | Intentional non-artist/program item | 0.98 | Terminus M belongs to a systematic timetable series (Terminus A through X and Greek-letter variants), indicating a labelled slot/location rather than a canonical performer. |
| Terminus Q | `unresolved` | Intentional non-artist/program item | 0.98 | This is one member of a systematic Terminus letter/symbol placeholder series in the lineup, not a corroborated artist identity. |
| Terminus T | `unresolved` | Intentional non-artist/program item | 0.97 | Within the supplied lineup, Terminus T is one of a systematic series of Terminus letter/symbol labels, indicating a location or program marker rather than an artist identity. |
| Terminus X | `unresolved` | Remain unresolved | 0.97 | Terminus X belongs to the same ambiguous lineup series and cannot safely be equated with Terminator X, Terminus B, or another similarly named act. |
| Terminus δ | `unresolved` | Intentional non-artist/program item | 0.99 | Terminus δ is one member of the lineup's systematic Terminus letter/Greek-letter series, so it is a timetable label rather than an artist billing. |
| Terminus ε | `unresolved` | Intentional non-artist/program item | 0.99 | The Greek-symbol suffix is part of the lineup's systematic Terminus placeholder series and should not be treated as an artist identity. |
| Tokyo Riddim Band (Live) | `needs_review` | Identity likely; canonical profile missing | 0.94 | Live is a qualifier and Tokyo Riddim Band is clearly documented as an act, but it has no matching canonical profile. |
| Tom Pardhy Life Celebration | `needs_review` | Intentional non-artist/program item | 0.88 | The wording denotes a life-celebration/memorial programme item rather than a performer billing. It should not create or attach an artist identity from punctuation or name-like text alone. |
| Vera | `needs_review` | Retry with better evidence | 0.42 | The common single-name billing cannot safely be assigned to Vera Heindel without independent or festival-specific corroboration. |
| Wayne Holland | `needs_review` | Identity likely; canonical profile missing | 0.96 | Two exact lineup occurrences and scene-specific evidence establish the relevant Wayne Holland DJ, but no canonical profile is supplied. |
| Yamuna Body Rolling | `unresolved` | Intentional non-artist/program item | 0.99 | Yamuna Body Rolling is a named bodywork/wellness practice and programmed activity, not an artist identity. |
| Yamuna Body Rolling with Gemma Nash | `needs_review` | Intentional non-artist/program item | 0.98 | This is a named wellness/bodywork session facilitated by Gemma Nash, not an artist performance billing. |
| Z@P (Live) | `unresolved` | Safe existing-profile override | 0.99 | Live is only a performance qualifier; the exact standalone Z@P lineup billing, canonical alias/profile data, and artist evidence all identify the existing Z@p profile. |

https://musicbrainz.org/artist/a1110b58-d013-4d62-8776-75bd0b6e29a2

## Operational notes

- This is a fixed snapshot; queue processing after the capture time may add or update records.
- The current CLI lists all statuses but has no approve/reject UI or machine-readable review reason.
- Retrying may reuse cached Tavily/DeepSeek results. A retry is not guaranteed to gather fresh evidence.
- Manual overrides are global for a billing key and propagate to every affected FestivalDO.
- The three talk billings are intentionally treated as program items here. If artist paths should include panel/talk participation, review them under a separate presenter-credit policy.
