# Safe UK policy for an online affiliate festival gear hub

**Research date:** 2026-08-11
**Scope:** A separate, online-only gear recommendation hub associated with an offline festival safety guide. This is a policy and source review, not legal advice. It assumes UK users and physical products bought from third-party retailers; requirements must be rechecked before launch and whenever an affiliate programme, app-store policy, product category or UK law changes.

## Executive recommendation

Proceed only with a **clearly separated, online affiliate hub for low-risk physical gear**. The offline safety guide must remain complete, non-commercial and fully usable without the hub.

The safest model is:

1. **Keep commerce out of safety content.** Do not place affiliate links, product cards, prices, retailer branding or purchase prompts in emergency actions, substance pages, first aid, welfare guidance or downloaded offline content. Offer one secondary route labelled **“Ad — online gear hub (affiliate links)”**, outside urgent flows. The app and guide must not imply that buying anything is necessary to be safe.
2. **Treat the whole hub as advertising, then label every commercial link again.** Use an upfront heading such as **“Advertisement — Gear hub”** and plain language: **“We earn commission if you buy through links marked ‘Ad’.”** Put **“Ad — affiliate link”** beside or immediately before every product CTA. Do not use only “affiliate”, “#aff”, “partner”, “in association with”, an icon, a footer or a terms page. CAP says affiliate content must be identifiable before engagement and that a bottom disclaimer or ambiguous “may earn” wording is unlikely to be enough.[S1][S2] CMA guidance likewise calls for clear, prominent, upfront and timely labelling and recommends “Ad” or “Advert”.[S5]
3. **Firewall rankings from commission.** Commission rate, retailer bounty and conversion probability must not affect eligibility, score, order, badges or editorial safety advice. Publish category-specific ranking criteria and the evidence date. Put paid placements in a separate, labelled advertising block; never blend them into “best” rankings. Objective comparisons must use material, relevant, verifiable and representative features.[S3]
4. **Do not show prices or stock in version 1.** Link to the retailer for the current total. If prices are later added, use an authorised live feed, show retrieval time and delivery caveats, and suppress stale data. CAP requires non-misleading price and availability claims; Amazon permits prices and availability only when Amazon serves them or they come through its Product Advertising API under its licence.[S3][S21]
5. **Do not run affiliate tracking without valid consent.** The ICO's current PECR guidance expressly says online-advertising storage/access technologies—including “ad affiliation”, measurement and performance—require consent and do not fall within an exception.[S7] Before consent, set no affiliate/advertising storage or access technologies and send no advertising identifiers. Make reject as easy as accept. On rejection, offer an untagged retailer link or no commercial link, not a degraded safety guide.
6. **Open the hub and merchants in the system browser.** The app should link only to the separate hub, without user or safety-page identifiers. The hub may then link, after disclosure and consent, directly to an allowlisted product detail page following the affiliate programme's format. Do not auto-redirect, cloak, shorten, frame or prefetch merchant links. Amazon specifically requires direct product links, prohibits obscuring the source URL, restricts tagged links in mobile apps, and prohibits framing Amazon in an in-app WebView.[S20][S21]
7. **Use a narrow product whitelist.** At launch:
   - **Eligible with controls:** dedicated sunscreen, hearing-protection earplugs, reputable power banks, drinking-water containers/hydration bladders, weatherproofing, lights, dry bags and similarly low-risk non-ingestible gear.
   - **Default ineligible:** supplements and ingestibles, hangover products, drug-test kits, diagnostic/monitoring/treatment devices, medicines, water-purification products and products whose value depends on a health, disease or absolute safety claim.
   - **Always prohibited:** illegal or controlled substances; cannabis/CBD/THC products; alcohol; tobacco, vapes and nicotine; prescription-only medicines; weapons and self-defence products; knives; pyrotechnics, flares, fuels and compressed-gas products; recalled/counterfeit goods; and any product barred by the retailer, affiliate network, venue or app store.
8. **Design for minors and situational vulnerability.** Festival users may be under 16, intoxicated, distressed, sleep-deprived, injured or urgently seeking reassurance. Use no scarcity countdowns, fear-based “buy this to stay safe” messaging, personalised targeting, buy-now-pay-later referrals or rankings based on sensitive guide activity. CAP prohibits exploiting children's vulnerability and direct purchase appeals to children; CMA says age and health status can affect whether a practice is aggressive or unfair.[S4][S6]
9. **Meet WCAG 2.2 AA and make disclosure perceivable.** Disclosure, consent, rankings and link purpose must work with keyboard, screen reader, zoom, large text and reduced motion; must not rely on colour; and must remain visible on small screens. Use text such as **“Ad — affiliate link to [retailer], opens externally”** as the accessible name.[S23]

## 1. Why separation is required

### 1.1 Regulatory boundary

Joining an affiliate scheme creates a payment arrangement to promote another trader's products. CAP says attributable links or codes are likely to be directly connected with the supply of goods, so the CAP Code applies. Where a page wholly concerns affiliate-linked products, the page is advertising in full. Where genuine editorial material contains only some affiliate links, the commercial product references, associated text and links are advertising.[S1]

That distinction makes mixed safety-and-commerce pages legally and ethically fragile. A disclaimer cannot turn a commercially influenced product recommendation back into independent safety guidance. CAP also states that both the advertiser and affiliate can remain responsible for compliant content.[S1]

**Policy boundary:**

| Offline safety guide | Online gear hub |
|---|---|
| Clinically/editorially governed; no commission objective | Commercial advertising surface |
| Complete in airplane mode | Explicitly requires internet |
| No products, merchant logos, prices, stock or tagged URLs | Whitelisted physical products only |
| No commercial analytics or affiliate SDK | Consent-gated affiliate measurement only |
| Emergency and harm-reduction actions | Advance-planning and convenience purchases |
| Sources chosen for public-safety value | Products scored under published commercial-conflict rules |

The only permitted bridge is a secondary, non-urgent link whose commercial nature is clear **before** it is opened. Do not place that bridge inside 999/emergency instructions, symptom checklists, overdose guidance, “what to do now” cards or product-independent advice.

### 1.2 Required user-facing wording

Recommended minimum wording on the app/guide link:

> **Ad — online gear hub (affiliate links)**
>
> Optional festival equipment. Requires internet. The safety guide does not require a purchase.

Recommended minimum wording at the top of every hub landing and category page:

> **Advertisement — Gear hub**
>
> We earn commission if you buy through links marked “Ad”. Commission does not influence our eligibility rules or ranking scores. Current price, stock, delivery and seller details are confirmed by the retailer.

Recommended product CTA:

> **Ad — view [product] at [retailer] (opens externally)**

If Amazon Associates is used, its current operating agreement also requires the site to state clearly and prominently:

> As an Amazon Associate I earn from qualifying purchases.

That programme statement is additional to, not a replacement for, the CAP-compliant page and link labels.[S20]

Avoid:

- “affiliate”, “#aff”, “collab”, “partner link” or an icon without “Ad”;
- “we may earn” where commission is in fact earned on qualifying purchases;
- disclosure after the first product, below the fold, behind “About”, in a footer only or in terms;
- a colour-only or hover-only indicator;
- wording that implies the retailer, festival, NHS, medical staff, Apple, Google or a public body endorses the hub.

CAP's affiliate guidance says advertising must be identifiable when the link and associated claim are encountered, and cites “(Ad)” before the relevant content as a likely acceptable approach.[S1] CMA says an audience should not have to scroll, resize, inspect a profile or already know about a relationship to recognise an ad.[S5]

## 2. Editorial independence, rankings and reviews

### 2.1 Mandatory conflict policy

Create a public methodology page and an internal decision record. Both must state:

- which retailers and affiliate networks can pay commission;
- the product universe considered and any merchant exclusions;
- all eligibility gates and category-specific score factors;
- whether products were physically tested, desk-reviewed or assessed from manufacturer documentation;
- when each model and claim was last checked;
- that commission rate, expected conversion and commercial campaign targets are excluded from scoring;
- how free samples, loans, gifts and sponsorships are handled;
- how errors, recalls, seller changes and complaints cause removal or re-review.

Commercial staff may negotiate programmes and maintain links, but must not change safety copy, the score, inclusion threshold or order. Editorial reviewers should not see commission rates while scoring. Keep an audit log of the initial score and each change.

### 2.2 Ranking rules

1. **Pass/fail safety gate first.** A product must have an identifiable model/SKU, traceable manufacturer or responsible seller, appropriate instructions, no relevant recall/alert, and the evidence required for every published objective claim.
2. **Score only comparable products.** “Best earplug” cannot compare a disposable foam plug with a custom clinical service without clearly defining the common need. CAP comparative claims must compare products meeting the same need or purpose and objectively compare material, relevant, verifiable and representative features.[S3]
3. **Use category-specific factors.** Examples are capacity and tested output for power banks, labelled attenuation/fit options for earplugs, SPF/UVA presentation and application format for sunscreen, or volume/cleanability/leak resistance for water containers. Do not use a universal score that obscures safety-critical differences.
4. **Do not call an item “best”, “safest”, “festival-proof”, “medical-grade”, “all-day” or “guaranteed” without evidence that substantiates the exact likely consumer interpretation.** CAP requires documentary evidence before publication for objective claims and prohibits exaggeration.[S3][S9]
5. **Sponsored positions are not rankings.** Put them in a distinct block headed “Advertisement — paid placement”. They do not receive “recommended”, “top pick” or an editorial score unless they independently pass and are shown in the ordinary ranking at the position produced by that score.
6. **No personalised ranking from safety-guide activity.** Reading pages on substances, menstruation, medication, mental health, disability or emergencies must never affect product selection, ordering or retailer links. Do not put article IDs, search terms or user IDs into affiliate subtags.

Amazon allows sub-tags to monitor links but says a sub-tag must never be associated with a specific end user for behavioural monitoring.[S21] The stricter hub policy is to use only coarse, static category/link identifiers and never identifiers derived from a person's guide activity.

### 2.3 Reviews, ratings and testimonials

The safest launch policy is **do not host user reviews and do not copy merchant star ratings or excerpts**.

If reviews are later introduced, the DMCC Act's fake-review rules require publishers to take reasonable and proportionate steps to prevent and remove fake reviews, concealed incentivised reviews and false or misleading review information. CMA expects a published policy, risk assessment, detection, investigation, sanctions and repeated effectiveness review.[S6] CAP also requires incentivised reviews to be identified, prohibits misleading selective publication or prominence, and requires evidence that testimonials are genuine.[S3]

Do not use reviews as substantiation for objective safety or health claims. Amazon additionally prohibits displaying its reviews or star ratings outside permitted API use.[S20]

## 3. Product claims and evidence

### 3.1 General claim standard

For every claim shown by the hub—even wording copied from a manufacturer or retailer—record:

- exact claim and likely consumer meaning;
- model/SKU and product version;
- source owner, document/version/date and URL or retained evidence;
- whether it is a manufacturer specification, applicable conformity document, independent test or editorial observation;
- important limitations, fit/use conditions and jurisdiction;
- reviewer and next review date.

CAP rule 3.7 requires documentary evidence **before** publication for objectively substantiable claims. Qualifications must be clear and cannot contradict the headline claim. Subjective opinions must not be presented as objective facts.[S3] An affiliate disclaimer or “not medical advice” notice does not cure a misleading claim.

Do not:

- copy a merchant title containing an unverified “medical”, “clinically proven”, “detox”, “anti-anxiety”, “hangover cure”, “prevents hearing damage”, “100% safe” or similar claim;
- infer quality or safety merely from sales volume, a marketplace badge, star rating, “CE/UKCA” text in an image or a seller's use of “certified”;
- convert a laboratory specification into a real-world guarantee;
- claim personal testing unless the exact retail model was tested under a documented protocol;
- imply that gear replaces shade, water access, sleep, medical attention, venue welfare, hearing breaks or emergency action.

### 3.2 Health products, supplements and medical devices

CAP applies a high level of scrutiny to medicines, medical devices and health-related products. Objective claims may require human trials; medicinal claims may be made only for appropriately licensed medicines or conforming medical devices. Ads must not discourage essential treatment, and claims of guaranteed efficacy, absolute safety or no side effects require proof.[S9]

Food and supplement health claims are separately constrained: only authorised nutrition/health claims meeting their conditions of use may be used; general health benefits require an accompanying specific authorised claim; and food ads cannot claim to prevent, treat or cure disease.[S10] CAP says a hangover and its symptoms are adverse medical conditions, so hangover-treatment claims are likely medicinal; food/drink/supplement claims to prevent, treat or cure a hangover are not acceptable.[S12]

MHRA states that medical devices placed on the Great Britain market must be registered with MHRA and satisfy the applicable marking/conformity route, with different rules in Northern Ireland and transitional acceptance of some CE-marked devices.[S13] A conformity mark is a regulatory route, not evidence for every advertising claim or an editorial guarantee that the product is “safe”.

## 4. Price, stock, seller and link freshness

### 4.1 Launch policy: omit prices and stock

Version 1 should show no price, discount percentage, “deal”, stock level, delivery date, scarcity badge or price-based “best value” claim. Use:

> Check current price, stock, seller and delivery at [retailer].

This avoids presenting stale commercial information as current. CAP requires price statements to relate to the featured product, include or properly explain compulsory charges, state delivery charges where applicable, and not exaggerate availability or benefits through “from” or “up to” wording. Marketers must not advertise at a price they have reasonable grounds to believe cannot be supplied in reasonable quantity and time.[S3]

### 4.2 Conditions if prices are introduced later

Prices may be enabled only when all of these are met:

- the affiliate programme explicitly permits display and provides the data through an authorised link/widget/API;
- the price is fetched at render time or within the programme's stricter permitted cache lifetime;
- the product model, variant, seller, condition and country match the displayed product;
- the UI shows **“Price checked [date/time/timezone]”** and **“Delivery and final total vary; confirm at retailer”**;
- stale, failed or ambiguous responses suppress the price rather than retaining the last value;
- discounts have a valid comparator and end time and are automatically removed at expiry;
- “best value” is recomputed from current like-for-like total costs, not list price alone.

Amazon's current linking requirements say product prices and availability vary and may be displayed only when Amazon serves that information or it is obtained through the Product Advertising API under the applicable licence. Limited-time promotion references must be removed when they expire.[S21]

### 4.3 Operational freshness

Recommended internal service levels (policy controls, not claims about legal minimums):

- validate destination, model and seller at least daily;
- check the OPSS recall/alert feed nightly and before adding a product;
- immediately suspend a model on a relevant safety alert, manufacturer warning or credible mismatch report;
- re-review claims after any model, packaging, formulation, marking or seller change and at least every six months;
- show a visible **“Reviewed [date]”** on each card;
- never automatically replace an unavailable model with a marketplace “similar item”.

OPSS publishes UK product recalls, safety reports and alerts and explains that serious/high-risk and recalled products appear in its public list.[S22]

## 5. Tracking, cookies and privacy

### 5.1 PECR position

The ICO's final 29 April 2026 guidance covers cookies, pixels, link decoration, navigational tracking, fingerprinting, web storage and app technologies. It says online advertising storage/access requires consent; the strictly necessary and statistical-purpose exceptions do not cover advertising. The listed advertising purposes include ad affiliation, measurement, performance and click-fraud detection.[S7]

Therefore the hub must not assume affiliate attribution is “strictly necessary” merely because commission funds the guide. It is not necessary from the user's perspective.[S7]

### 5.2 Consent design

Before consent:

- load only technologies genuinely covered by a PECR exception;
- do not load affiliate pixels, advertising SDKs, tracking redirects or merchant embeds;
- do not decorate links with user identifiers;
- do not prefetch or preconnect to merchant/affiliate endpoints where that stores or accesses device information for advertising;
- keep all safety content and non-commercial hub methodology readable.

The first layer should offer equally prominent **“Accept affiliate measurement”**, **“Reject”** and **“Manage choices”** controls. Advertising/affiliate toggles are off by default. Explain the named affiliate networks/retailers, purposes, technology duration and data flows in clear language. Withdrawal must be as easy as acceptance and must stop future non-exempt storage/access. ICO says consent requires positive action, purpose-specific choices, named third parties and a mechanism that technically respects the choice; silence or continued browsing is not consent.[S7]

When the user rejects:

- use an ordinary untagged direct retailer URL if programme terms and privacy review permit it; or
- provide the product name/model for independent search without an active commercial link.

Do not block the guide, hide rankings or repeatedly nag. A retailer reached after an intentional external click has its own privacy responsibilities; the hub should warn that the user is leaving and link to the retailer's privacy information where practical. ICO says services linking to external sites should make clear that the destination may use storage/access technologies and that the referring service is not responsible for that site's technologies.[S7]

### 5.3 Privacy minimisation

Do not create accounts for the hub. Do not send or store:

- app advertising IDs, precise location, contacts or device fingerprint;
- a stable cross-site or cross-device ID;
- safety-page reading history, substance searches, medical interests or emergency use;
- free-text search terms in affiliate URLs;
- individual purchase histories or inferred health/vulnerability profiles.

The privacy notice should identify the operator, each controller/processor as appropriate, data and source, purposes and lawful bases, recipients, retention, international transfers, rights, consent withdrawal and complaint route. ICO requires clear, comprehensive, concise and accessible information about each storage/access technology, its purpose, third parties and duration.[S7]

If the native app itself shares data for advertising or tracks across apps/sites, Apple requires disclosure and explicit App Tracking Transparency permission; it also prohibits using health/fitness/medical-context data for advertising or marketing. The recommended design avoids this by having no affiliate SDK or merchant link tracking in the app and by passing no user identifier to the web hub.[S15]

## 6. Deep links, app-store boundaries and offline behaviour

### 6.1 Link policy

Every product link must:

- require an intentional tap after the user can see the “Ad” label, merchant and external-link purpose;
- use HTTPS and an allowlisted affiliate/merchant domain;
- go directly to the identified model's product detail page;
- preserve visible merchant identity; no URL cloaking or generic shorteners;
- use only programme-supplied formats and identifiers;
- contain no person-specific subtag, safety article ID or sensitive search term;
- be continuously checked for product swaps, redirects, dead pages and seller changes.

Do not auto-open merchants, use pop-unders, trigger links while scrolling, make the dismiss control an ad click, pre-load a merchant WebView or frame checkout. Amazon requires an affirmative customer click, direct product links and correct formatting, prohibits obscuring the referring URL, and prohibits framing Amazon in an integrated WebView.[S20][S21]

### 6.2 Native app boundary

The app should open the **hub landing page** in the user's system browser. It should not contain tagged Amazon product links. Amazon states that purchases through Special Links in a mobile application are disqualified unless the app is approved and links are served through specified Amazon APIs/tools; its participation rules also restrict tagged links in client software and WebViews.[S20]

Apple permits payment outside in-app purchase for physical goods consumed outside the app, but it rejects apps designed predominantly to display ads and says apps should not primarily be marketing material, advertisements or collections of links.[S15] The substantive offline festival guide must remain the app's core utility.

Google Play likewise excludes physical goods from Play Billing.[S16] If children are among the declared target audience, Google says an app must not have the primary purpose of driving affiliate traffic, and all commercial content is subject to Families requirements.[S17] Keep Play Console target-audience, health-content and Data safety declarations accurate; a festival safety guide with health information may fall within Google's Health Content and Services policy even if the affiliate hub is separate.[S18]

### 6.3 Offline behaviour

In airplane mode or poor connectivity:

- every safety page, emergency action, internal search and local asset continues to work;
- the hub route shows only **“Gear hub unavailable offline”** and a dismiss/back action;
- no cached price, “in stock”, discount or retailer claim is shown as current;
- no repeated connection attempts, merchant prefetch or tracking queue is created;
- no urgent content is obscured by a commercial network error;
- reconnecting does not automatically open the hub or merchant.

A static, non-commercial packing checklist may remain in the guide, but it must use generic categories (“hearing protection”, “refillable bottle”), not ranked models, merchant names or product claims.

## 7. Minors, vulnerable users and accessibility

### 7.1 Children and mixed audiences

CAP defines a child as under 16. Ads addressed to or featuring children must not exploit credulity, vulnerability or lack of experience, encourage unsafe copying, or directly appeal to children to buy or persuade adults to buy. Distance-selling marketers must take care not to promote unsuitable products in youth media.[S4]

The hub should therefore:

- carry no age-restricted product;
- use no child-directed purchase wording, influencers, collectible rewards or “ask your parents” CTA;
- use no products or imagery that glamorise controlled substances, alcohol, nicotine or risky behaviour;
- avoid expensive “must-have” bundles and social-status pressure;
- not infer age through covert tracking.

If the service is likely to be accessed by under-18s, the ICO Children's Code calls for best interests as a primary design consideration, a DPIA, age-appropriate transparency, high privacy by default, data minimisation, limited sharing, profiling off by default and no nudges to weaken privacy.[S8] For this low-data hub, applying those protections to everyone is safer than collecting extra data for age assurance.

Google Play's Families policy requires accurate target-audience declarations. For apps targeting both children and adults, child/unknown-age users cannot receive personalised ads; commercial content must be clearly distinguishable; affiliate traffic cannot be the app's primary purpose; and child-directed ads have SDK/content/format restrictions.[S17] The recommended architecture uses no in-app affiliate feed or ad SDK.

### 7.2 Situational vulnerability

The CMA's current unfair-commercial-practices guidance says the effect of aggressive or unfair practices can depend on the consumer's situation, including age or health status, and traders likely to deal with vulnerable consumers must consider that impact.[S6] Festival users may be vulnerable temporarily even if they are normally confident shoppers.

Prohibit:

- “you need this to survive/stay safe” or fear of illness/injury as a sales device;
- countdowns, false scarcity, repeated prompts and default acceptance;
- purchase prompts after viewing emergency or substance-risk content;
- personalised products based on inferred health, drug use, disability or distress;
- credit, gambling, prize, mystery-box or buy-now-pay-later referrals;
- claims that a purchase substitutes for medical/welfare advice.

### 7.3 Accessibility policy

Conform the hub to **WCAG 2.2 level AA** and test with assistive technology.[S23] At minimum:

- semantic headings, lists, comparison tables and buttons;
- full keyboard operation and visible focus;
- text alternatives for product images;
- sufficient contrast in default, hover, focus and disabled states;
- disclosure that remains textually explicit without colour or icons;
- meaningful link names including “Ad”, merchant and external destination;
- reflow at 320 CSS px, text zoom and no horizontal product-card trap;
- consent choices that are equally reachable and understandable;
- no focus-stealing countdown, auto-carousel or unexpected navigation;
- error/status messages announced to screen readers;
- at least 44 by 44 CSS-pixel touch targets as the product's design policy;
- plain language, short sentences and no reliance on hover/tooltips.

WCAG conformance supports but does not by itself determine compliance with the Equality Act 2010 duty to make reasonable adjustments for disabled users.[S24]

## 8. Product-category decisions

### 8.1 Decision table

| Category | Launch decision | Required conditions / reason |
|---|---|---|
| **Supplements, vitamins, electrolyte supplements, nootropics, sleep/energy products** | **Ineligible** | Exclude all ingestible supplements at launch. Health/nutrition claims are tightly constrained, disease claims are prohibited for foods, and formulation/interaction risks are disproportionate to affiliate value.[S10] Google Play also prohibits promotion/sale of unapproved substances, dangerous supplement ingredients and false health claims.[S18] Ordinary hydration advice remains editorial and product-neutral. |
| **Hangover products or “recovery” ingestibles** | **Prohibited** | CAP says hangover symptoms are adverse medical conditions; claims to treat/prevent them are likely medicinal and food/supplement disease-treatment claims are not acceptable.[S12] They also risk encouraging alcohol use and exploiting vulnerable users. |
| **Drug-test kits: reagent kits, fentanyl/nitazene strips, urine/saliva tests** | **Ineligible for affiliate promotion** | Do not monetise them. UK government guidance says strips may miss below-limit drugs, variants, poorly dissolved samples or interfered tests; do not indicate quantity; and do not detect other harmful drugs.[S14] Commercial “accurate/safe/purity” copy is especially likely to mislead. Keep neutral, source-cited editorial information and official service links separate. |
| **Medical devices generally** | **Default ineligible; narrow future whitelist only** | Do not promote diagnostic, monitoring or treatment devices (including products whose appeal rests on identifying or managing a condition). A future exception may cover basic sealed external first-aid supplies only after model-level MHRA registration/marking, instructions, claim and recall checks. GB and NI requirements differ.[S9][S13] |
| **Medicines, naloxone, painkillers, antihistamines and herbal remedies** | **Prohibited from affiliate hub** | Keep medicines and emergency antidotes in non-commercial official guidance/referral paths. Never monetise access to essential treatment. Prescription-only medicines must not be advertised to the public, and health advertising must not discourage essential treatment.[S9] |
| **Dedicated sunscreen** | **Eligible with conditions** | Use reputable UK retailers and dedicated sunscreen, not an ordinary moisturiser presented as a substitute. Show only verified label facts such as SPF, UVA presentation, format and volume. Do not claim complete/all-day protection, prevention of cancer/ageing, reversal of damage or that sunscreen replaces shade, clothing and correct reapplication. CAP treats sunscreen generally as a cosmetic and warns against exaggerated or socially irresponsible sun-safety claims.[S11] |
| **Earplugs sold as hearing protection** | **Eligible with conditions** | Require a traceable manufacturer/seller, applicable PPE conformity information and model-specific labelled attenuation/fit instructions. Compare like with like. Do not claim universal fit, complete protection, “medical grade”, prevention of hearing loss or that plugs remove the need for breaks/distance from loud sound. Suspend if the model or conformity evidence changes. UK product-supply guidance treats hearing protection as PPE and requires the applicable conformity, information and traceability controls.[S25] |
| **Power banks and charging cables** | **Eligible with conditions** | Only identifiable models from established manufacturers and traceable UK sellers. Verify model capacity/output/ports from documentation, check OPSS recalls, and include neutral battery handling/instructions. No unbranded marketplace substitutions, “lasts all weekend”, waterproof, airline-safe or absolute fire-safety claims without exact evidence. Power banks are never placed next to emergency advice as a safety necessity.[S22] |
| **Water bottles, collapsible containers and hydration bladders** | **Eligible with conditions** | Products must be sold for drinking-water/food-contact use by a traceable seller. Limit copy to verified capacity, material, closure, dimensions, cleaning and temperature limits. Do not make detox, antimicrobial, BPA-related health, sterilisation or “keeps water safe” claims beyond evidence. Remind users to follow venue refill rules; do not suggest a container guarantees hydration. |
| **Water filters, purification tablets or UV sterilisers** | **Ineligible at launch** | Failure can cause illness and claims depend on organism, water condition, dose, contact time and maintenance. They require a separate evidence/regulatory process, not a generic gear card. |
| **Low-risk non-ingestible gear** | **Eligible with controls** | Examples: sun hats, ponchos, dry bags, weatherproof phone pouches, torches/headlamps, ordinary charging accessories, blankets, tent accessories and reusable tableware. Use objective specs, traceable sellers, recall checks and venue-specific caveats. |
| **High-risk camping/event equipment** | **Prohibited or manual exception only** | No stoves, liquid/gas fuel, compressed cylinders, generators, high-power lasers, flares, fireworks, blades, weapons, self-defence sprays or unsafe electrical products. Venue rules and app/store dangerous-product rules make these unsuitable.[S15][S19] |
| **Alcohol, tobacco, vapes/nicotine, cannabis/CBD/THC, illegal/controlled drugs** | **Prohibited** | No product, accessory whose primary purpose is consumption, prominent promotion or retailer link. Apple and Google prohibit or restrict facilitating controlled-substance/tobacco sales and drug promotion, especially to minors.[S15][S19] |
| **Recalled, counterfeit, grey-market or untraceable products** | **Prohibited** | Remove immediately. Do not rely on marketplace availability as evidence of lawful/safe supply. Use OPSS and relevant MHRA/FSA alert channels.[S22] |

### 8.2 Specific answers

- **Supplements:** **No.** Exclude the whole class at launch, including hangover, energy, sleep, “immune”, electrolyte-supplement and nootropic products. The clean boundary is no ingestibles.
- **Drug-test kits:** **No affiliate eligibility.** Neutral harm-reduction information may explain limitations and link to official/non-commercial services.
- **Medical devices:** **No by default.** Consider only a documented whitelist of basic low-risk first-aid supplies in a later phase; exclude diagnostic, monitoring and treatment devices.
- **Sunscreen:** **Yes, conditionally.** Dedicated sunscreen from traceable UK supply, conservative label-level claims and no weakening of sun-safety advice.
- **Earplugs:** **Yes, conditionally.** Hearing-protection products with model-level conformity/attenuation evidence and prominent fit/use limitations.
- **Power banks:** **Yes, conditionally.** Identifiable reputable models, objective specifications, recall monitoring and no absolute battery-safety or duration claims.
- **Water containers:** **Yes, conditionally.** Drinking-water/food-contact intended products with factual capacity/material/cleaning claims; exclude purification and health claims.
- **Similar gear:** Eligible only when it is low-risk, non-ingestible, legal at common festival venues, traceable, objectively describable and independent of a medical or absolute safety claim.

## 9. Governance and launch controls

### 9.1 Required roles and records

- **Safety editorial owner:** controls all offline guide content and can veto commercial adjacency.
- **Affiliate/commercial owner:** manages programmes and links but cannot score products.
- **Claims reviewer:** approves the evidence record for every objective, comparative or health-adjacent claim.
- **Privacy owner:** maintains the data map, PECR assessment, consent platform, DPIA and partner contracts.
- **Accessibility reviewer:** verifies WCAG 2.2 AA and disclosure/consent usability.
- **Takedown owner:** can immediately disable a product, merchant, category or network.

Retain the product record, evidence, score, disclosure version, destination history, retailer/programme terms, consent configuration, accessibility result, recall checks, complaints and takedown decisions.

### 9.2 Pre-launch checklist

- [ ] The offline guide passes its full airplane-mode test with the hub unreachable.
- [ ] No affiliate URL, retailer SDK, merchant image, commission metadata or product price is bundled into offline safety content.
- [ ] The app-to-hub link says “Ad” before opening and is absent from urgent flows.
- [ ] Every hub page has the page-level advertisement disclosure before the first product.
- [ ] Every commercial CTA has an accessible “Ad — affiliate link” label.
- [ ] Amazon's required associate statement is present if Amazon is used.
- [ ] Commission rates are absent from ranking inputs and reviewer screens.
- [ ] Every score can be reproduced from the published criteria and retained evidence.
- [ ] No user reviews/ratings are displayed, or the CMA fake-review controls are operational.
- [ ] No prices or stock are shown; if later enabled, authorised live-data and stale-suppression tests pass.
- [ ] Reject and accept are equally prominent; all advertising technologies are off before consent.
- [ ] With rejection, tagged URLs, affiliate cookies/pixels and advertising identifiers are absent.
- [ ] Withdrawal is as easy as acceptance and is respected technically.
- [ ] No guide page ID, health/substance interest, user ID or free-text search appears in outbound URLs.
- [ ] Links require a tap, use allowlisted HTTPS destinations and do not auto-redirect, cloak or frame merchants.
- [ ] Amazon links exist only on the approved web hub unless the mobile app separately satisfies Amazon's mobile-app policy.
- [ ] OPSS/MHRA checks and rapid takedown are operational.
- [ ] Supplement, drug-test, medicine, high-risk device and prohibited-category filters are enforced.
- [ ] Child/mixed-audience and Play Console declarations are accurate.
- [ ] WCAG 2.2 AA tests cover disclosure, consent, comparison tables, cards, errors and external navigation.
- [ ] Apple/Google review notes describe the online commercial hub and confirm that the app's core offline guide is not paywalled or ad-dependent.

## 10. Primary and first-party sources

All web sources were accessed 2026-08-11.

**[S1] ASA/CAP.** [Online Affiliate Marketing](https://www.asa.org.uk/advice-online/affiliate-marketing.html). CAP AdviceOnline guidance on scope, separation of editorial and affiliate material, pre-engagement disclosure, link-level labelling, ambiguous disclaimers and shared responsibility.

**[S2] Committee of Advertising Practice.** [CAP Code, Section 2: Recognition of marketing communications](https://www.asa.org.uk/type/non_broadcast/code_section/02.html). Rules 2.1, 2.3 and 2.4 on obvious identification, commercial intent and advertorials.

**[S3] Committee of Advertising Practice.** [CAP Code, Section 3: Misleading advertising](https://www.asa.org.uk/type/non_broadcast/code_section/03.html). Rules on material information, substantiation, qualifications, prices, availability, comparisons, incentivised reviews and testimonials.

**[S4] Committee of Advertising Practice.** [CAP Code, Section 5: Children](https://www.asa.org.uk/type/non_broadcast/code_section/05.html). Definition of child and rules on harm, unsafe practices, credulity, pressure and direct purchase appeals.

**[S5] Competition and Markets Authority.** [Social media endorsements: guidance for content creators](https://www.gov.uk/government/publications/social-media-endorsements-guidance-for-content-creators/social-media-endorsements-being-transparent-with-your-followers), updated 3 September 2025; and [guidance for brands](https://www.gov.uk/government/publications/reviews-and-social-media-endorsements-guidance-for-businesses-and-brands/social-media-endorsements-guidance-for-brands), updated 28 August 2025. Current first-party labelling guidance under the DMCC Act context.

**[S6] Competition and Markets Authority.** [What businesses need to know about unfair commercial practices](https://www.gov.uk/government/publications/what-businesses-need-to-know-about-unfair-commercial-practices/what-businesses-need-to-know-about-unfair-commercial-practices), updated 3 December 2025; and [Short guide for businesses publishing consumer reviews](https://www.gov.uk/government/publications/fake-reviews/short-guide-for-businesses-publishing-consumer-reviews-and-complying-with-consumer-protection-law), published 4 April 2025. DMCC Act fairness, material omissions, vulnerability and fake-review controls.

**[S7] Information Commissioner's Office.** [Guidance on the use of storage and access technologies](https://ico.org.uk/for-organisations/direct-marketing-and-privacy-and-electronic-communications/guidance-on-the-use-of-storage-and-access-technologies/), finalised 29 April 2026; especially [exceptions](https://ico.org.uk/for-organisations/direct-marketing-and-privacy-and-electronic-communications/guidance-on-the-use-of-storage-and-access-technologies/what-are-the-exceptions/), [compliance and external links](https://ico.org.uk/for-organisations/direct-marketing-and-privacy-and-electronic-communications/guidance-on-the-use-of-storage-and-access-technologies/how-do-we-comply-with-the-pecr-rules/), [consent](https://ico.org.uk/for-organisations/direct-marketing-and-privacy-and-electronic-communications/guidance-on-the-use-of-storage-and-access-technologies/how-do-we-manage-consent-in-practice/) and [online advertising](https://ico.org.uk/for-organisations/direct-marketing-and-privacy-and-electronic-communications/guidance-on-the-use-of-storage-and-access-technologies/how-do-the-rules-apply-to-online-advertising/).

**[S8] Information Commissioner's Office.** [Age appropriate design: code standards](https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/childrens-information/childrens-code-guidance-and-resources/age-appropriate-design-a-code-of-practice-for-online-services/code-standards/). Best interests, DPIA, age-appropriate transparency, high privacy, minimisation, sharing, profiling and nudge standards.

**[S9] Committee of Advertising Practice.** [CAP Code, Section 12: Medicines, medical devices, health-related products and beauty products](https://www.asa.org.uk/type/non_broadcast/code_section/12.html). Health-claim evidence, licensing/conformity, essential treatment and absolute-safety rules.

**[S10] Committee of Advertising Practice.** [CAP Code, Section 15: Food, food supplements and associated health or nutrition claims](https://www.asa.org.uk/type/non_broadcast/code_section/15.html). Authorised-claim, conditions-of-use and disease-treatment restrictions.

**[S11] ASA/CAP.** [Beauty and Cosmetics: Sunscreen](https://www.asa.org.uk/advice-online/beauty-and-cosmetics-sunscreen.html), 28 January 2025. Classification, efficacy substantiation, dedicated-sunscreen presentation and social-responsibility guidance.

**[S12] ASA/CAP.** [Health: Hangover](https://www.asa.org.uk/advice-online/health-hangover.html), 21 May 2025. Medicinal status of hangover-treatment claims, food/supplement restrictions and responsibility concerns.

**[S13] Medicines and Healthcare products Regulatory Agency.** [Regulating medical devices in the UK](https://www.gov.uk/guidance/regulating-medical-devices-in-the-uk), updated 20 February 2026. GB/NI scope, MHRA registration, UKCA and CE transitional routes.

**[S14] UK Joint Combating Drugs Unit / Office for Health Improvement and Disparities / National Police Chiefs' Council.** [Local preparedness for synthetic opioids in England](https://www.gov.uk/government/publications/local-preparedness-for-synthetic-opioids-in-england/local-preparedness-for-synthetic-opioids-in-england-accessible), updated 23 June 2025. Paragraphs 83–84 describe test-strip false negatives, variant, dissolution, interference, target and quantity limits.

**[S15] Apple.** [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/), updated 8 June 2026. Relevant provisions include 1.4.3 (substances), 3.1.3(e) (physical goods), 3.2.2 and 4.2 (ad-dominant/marketing apps), and 5.1 (privacy, tracking, health data and children).

**[S16] Google Play.** [Payments policy](https://support.google.com/googleplay/android-developer/answer/9858738). Physical goods and physical services are outside required Play Billing.

**[S17] Google Play.** [Google Play Families Policies](https://support.google.com/googleplay/android-developer/answer/9893335). Target-audience declarations, affiliate-traffic limitation, child data, commercial content, ad distinction and mixed-audience requirements.

**[S18] Google Play.** [Health Content and Services](https://support.google.com/googleplay/android-developer/answer/16679511). Health declaration/privacy requirements, medical-device proof/disclaimers, prescription drugs, unapproved substances/supplements and misleading health claims.

**[S19] Google Play.** [Inappropriate Content](https://support.google.com/googleplay/android-developer/answer/9878810) and [Illegal Activities](https://support.google.com/googleplay/android-developer/answer/9878877). Dangerous products, marijuana, tobacco/nicotine, alcohol/minors and illegal-drug sales/promotion.

**[S20] Amazon UK Associates.** [Associates Program Operating Agreement](https://affiliate-program.amazon.co.uk/help/operating/agreement), updated 15 October 2025; [Programme Participation Requirements](https://affiliate-program.amazon.co.uk/help/operating/participation/). Required associate statement, Special Links, mobile-app restrictions, direct user action, review use, URL/source visibility and WebView restrictions.

**[S21] Amazon UK Associates.** [Programme Linking Requirements](https://affiliate-program.amazon.co.uk/help/operating/linking). Direct product deep links, no misleading claims/shortening, user-level subtag prohibition, expiring promotion removal and authorised price/availability display.

**[S22] Office for Product Safety and Standards.** [Product Recalls and Alerts](https://www.gov.uk/guidance/product-recalls-and-alerts), updated 28 July 2026. Official UK recall, safety report and safety alert route.

**[S23] World Wide Web Consortium.** [Web Content Accessibility Guidelines (WCAG) 2.2](https://www.w3.org/TR/WCAG22/), W3C Recommendation. Normative accessibility success criteria and conformance model.

**[S24] UK Parliament.** [Equality Act 2010, section 20](https://www.legislation.gov.uk/ukpga/2010/15/section/20) and [section 29](https://www.legislation.gov.uk/ukpga/2010/15/section/29). Reasonable-adjustment and services provisions; applicability to a particular operator requires legal assessment.

**[S25] Office for Product Safety and Standards.** [Personal Protective Equipment (Enforcement) Regulations 2018: Great Britain guidance](https://www.gov.uk/government/publications/personal-protective-equipment-enforcement-regulations-2018/regulation-2016425-and-the-personal-protective-equipment-enforcement-regulations-2018-great-britain); and Health and Safety Executive, [PPE product safety and supply](https://www.hse.gov.uk/ppe/product-safety-and-supply.htm). First-party product-supply, conformity, marking, information and traceability guidance for PPE, including hearing protection.
