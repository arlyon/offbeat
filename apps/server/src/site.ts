type SitePage = "home" | "support" | "privacy";

const supportUrl = "https://github.com/arlyon/offbeat/issues";

const styles = `
:root{color-scheme:dark;--bg:#0b0b0c;--panel:#111114;--fg:#f4f1ee;--muted:#969196;--line:#38343d;--pink:#ff2d8f;--cyan:#37d6c0;--amber:#ffb020;font-family:Helvetica,Arial,sans-serif}
*{box-sizing:border-box}html{background:var(--bg);scroll-behavior:smooth}body{margin:0;background:var(--bg);color:var(--fg)}a{color:inherit}a:focus-visible{outline:2px solid var(--cyan);outline-offset:4px}.mono{font-family:"JetBrains Mono","SFMono-Regular",monospace;text-transform:uppercase;letter-spacing:.1em}.shell{width:min(1160px,calc(100% - 40px));margin:0 auto}.nav{height:76px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px dotted var(--line)}.brand{display:flex;align-items:center;gap:13px;text-decoration:none;font-size:19px;font-weight:800;letter-spacing:.05em}.mark{display:grid;grid-template-columns:5px 5px 5px;align-items:end;gap:3px;width:21px;height:25px}.mark i{display:block;background:var(--fg)}.mark i:nth-child(1){height:13px}.mark i:nth-child(2){height:25px}.mark i:nth-child(3){height:8px;background:var(--pink)}.slashes{color:var(--pink)}.nav-links{display:flex;gap:24px}.nav-links a{color:var(--muted);font:700 12px "JetBrains Mono","SFMono-Regular",monospace;text-decoration:none;text-transform:uppercase;letter-spacing:.09em}.nav-links a:hover{color:var(--fg)}.hero{display:grid;grid-template-columns:1.1fr .9fr;gap:64px;align-items:center;min-height:660px;padding:72px 0;border-bottom:1px dotted var(--line)}.eyebrow{color:var(--pink);font:700 12px "JetBrains Mono","SFMono-Regular",monospace;text-transform:uppercase;letter-spacing:.12em}.hero h1{max-width:760px;margin:22px 0 24px;font-size:clamp(56px,7.5vw,104px);line-height:.91;letter-spacing:-.055em}.hero p{max-width:610px;margin:0;color:#c4bfc3;font-size:20px;line-height:1.55}.actions{display:flex;flex-wrap:wrap;gap:12px;margin-top:36px}.button{display:inline-flex;min-height:48px;align-items:center;justify-content:center;padding:0 20px;border:1px solid var(--pink);background:var(--pink);color:#09090a;font:800 12px "JetBrains Mono","SFMono-Regular",monospace;text-decoration:none;text-transform:uppercase;letter-spacing:.08em}.button.secondary{background:transparent;color:var(--fg);border-color:var(--line)}.button:hover{filter:brightness(1.12)}.timeline{border:1px solid var(--line);background:var(--panel);box-shadow:14px 14px 0 #18151a}.timeline-head{display:flex;justify-content:space-between;padding:18px;border-bottom:1px dotted var(--line);font-size:12px}.time-row{display:grid;grid-template-columns:70px 1fr;min-height:100px;border-bottom:1px dotted var(--line)}.time-row:last-child{border-bottom:0}.time{padding:18px 12px;color:var(--muted);font:700 12px "JetBrains Mono","SFMono-Regular",monospace}.set{margin:10px 10px 10px 0;padding:18px;border-left:3px solid var(--pink);background:#351020}.set.cyan{border-color:var(--cyan);background:#102825}.set.amber{border-color:var(--amber);background:#30230e}.set strong{display:block;font-size:17px}.set span{display:block;margin-top:8px;color:var(--muted);font:11px "JetBrains Mono","SFMono-Regular",monospace}.section{padding:96px 0;border-bottom:1px dotted var(--line)}.section h2,.page-head h1{max-width:850px;margin:14px 0 18px;font-size:clamp(42px,5vw,72px);line-height:.98;letter-spacing:-.04em}.lead{max-width:760px;color:#c4bfc3;font-size:19px;line-height:1.65}.grid{display:grid;grid-template-columns:repeat(3,1fr);gap:1px;margin-top:48px;background:var(--line);border:1px solid var(--line)}.card{min-height:250px;padding:30px;background:var(--bg)}.card .number{color:var(--pink);font:700 11px "JetBrains Mono","SFMono-Regular",monospace}.card h3{margin:50px 0 12px;font-size:25px}.card p{margin:0;color:var(--muted);line-height:1.6}.statement{display:grid;grid-template-columns:1fr 1fr;gap:60px;align-items:end}.statement blockquote{margin:0;font-size:clamp(35px,4vw,62px);font-weight:800;line-height:1.04;letter-spacing:-.035em}.statement p{margin:0;color:var(--muted);font-size:17px;line-height:1.7}.page-head{padding:88px 0 64px;border-bottom:1px dotted var(--line)}.content{width:min(820px,calc(100% - 40px));margin:0 auto;padding:72px 0 110px}.content h2{margin:56px 0 14px;font-size:28px}.content h2:first-child{margin-top:0}.content p,.content li{color:#c4bfc3;font-size:17px;line-height:1.7}.content li+li{margin-top:8px}.content a{color:var(--cyan)}.notice{padding:22px;border-left:3px solid var(--pink);background:var(--panel)}.faq{border-top:1px dotted var(--line)}.faq article{padding:30px 0;border-bottom:1px dotted var(--line)}.faq h2{margin:0 0 10px;font-size:22px}.faq p{margin:0}.footer{display:flex;justify-content:space-between;gap:24px;padding:34px 0;color:var(--muted);font:11px "JetBrains Mono","SFMono-Regular",monospace;text-transform:uppercase;letter-spacing:.08em}.footer div{display:flex;gap:18px}.footer a{text-decoration:none}.footer a:hover{color:var(--fg)}
@media(max-width:800px){.shell{width:min(100% - 28px,1160px)}.nav-links{gap:14px}.hero{grid-template-columns:1fr;min-height:auto;padding:64px 0}.hero h1{font-size:58px}.timeline{box-shadow:8px 8px 0 #18151a}.grid{grid-template-columns:1fr}.card{min-height:210px}.statement{grid-template-columns:1fr}.section{padding:70px 0}.footer{flex-direction:column}.content{width:min(100% - 28px,820px)}.nav-links a:first-child{display:none}}
`;

function mark() {
	return '<span class="mark" aria-hidden="true"><i></i><i></i><i></i></span>';
}

function layout(page: SitePage, title: string, description: string, content: string) {
	const canonical = page === "home" ? "" : `/${page}`;
	return `<!doctype html>
<html lang="en-GB">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${title}</title>
<meta name="description" content="${description}">
<meta name="theme-color" content="#0b0b0c">
<link rel="canonical" href="https://offbeat.arlyon.dev${canonical}">
<style>${styles}</style>
</head>
<body>
<header class="shell nav">
<a class="brand" href="/">${mark()}<span>OFFBEAT<span class="slashes">//</span></span></a>
<nav class="nav-links" aria-label="Main navigation">
<a href="/#features">Features</a><a href="/support">Support</a><a href="/privacy">Privacy</a>
</nav>
</header>
<main>${content}</main>
<footer class="shell footer"><span>© 2026 Offbeat</span><div><a href="/support">Support</a><a href="/privacy">Privacy</a><a href="https://github.com/arlyon/offbeat">Source</a></div></footer>
</body>
</html>`;
}

function homePage() {
	return layout(
		"home",
		"Offbeat — Festival companion",
		"A local-first festival companion for lineups, groups, chat and offline coordination.",
		`<section class="shell hero">
<div><div class="eyebrow">Local-first festival companion</div><h1>Keep your festival on track.</h1><p>Lineups, personal schedules, groups and chat—designed to stay useful when the signal gives up.</p><div class="actions"><a class="button" href="#features">Explore features</a><a class="button secondary" href="/support">Get support</a></div></div>
<div class="timeline" aria-label="Example festival schedule"><div class="timeline-head mono"><span>Friday 14</span><span>17:35 live</span></div><div class="time-row"><span class="time">17:00</span><div class="set"><strong>Kelly Lee Owens</strong><span>The Village · 17:00–18:30</span></div></div><div class="time-row"><span class="time">18:45</span><div class="set cyan"><strong>Ben UFO</strong><span>Airbase · 18:45–20:45</span></div></div><div class="time-row"><span class="time">20:45</span><div class="set amber"><strong>Overmono</strong><span>Junkyard · 20:45–22:00</span></div></div></div>
</section>
<section class="section" id="features"><div class="shell"><div class="eyebrow">Built for the field</div><h2>Everything you need. Nothing that needs perfect reception.</h2><div class="grid"><article class="card"><span class="number">01 // PLAN</span><h3>See the whole lineup</h3><p>Browse days and stages on a visual timeline. Star sets and catch clashes before they happen.</p></article><article class="card"><span class="number">02 // NOW</span><h3>Know what is on</h3><p>See who is playing, what starts next and where your saved schedule takes you.</p></article><article class="card"><span class="number">03 // TOGETHER</span><h3>Find your people</h3><p>Create groups, invite by QR, share manual check-ins and keep the conversation moving.</p></article></div></div></section>
<section class="section"><div class="shell statement"><blockquote>Festival networks are unreliable. Your plan should not be.</blockquote><p>Offbeat caches festival information on your device and uses available peer and internet routes to synchronise. Previously loaded schedules stay available when connectivity does not.</p></div></section>`,
	);
}

function supportPage() {
	return layout(
		"support",
		"Offbeat Support",
		"Help with Offbeat festival lineups, groups, offline use and TestFlight builds.",
		`<header class="shell page-head"><div class="eyebrow">Support</div><h1>How can we help?</h1><p class="lead">Offbeat is in active testing. Search the common questions below or report a reproducible problem on GitHub.</p></header><section class="content"><div class="notice"><strong>Report a problem</strong><p>Open a GitHub issue with your device model, operating-system version, app build and the steps that caused the problem. Do not include private group keys, passkeys or other secrets.</p><a class="button" href="${supportUrl}">Open GitHub issues</a></div><div class="faq"><article><h2>Why is a festival missing?</h2><p>Festival availability depends on published lineup data. You can add a supported public Clashfinder from the Festivals screen.</p></article><article><h2>Does Offbeat work offline?</h2><p>Previously loaded festival schedules and local planning remain available offline. New data and synchronisation resume when a compatible route becomes available.</p></article><article><h2>Why does Offbeat request camera access?</h2><p>The camera is used only when you open the QR scanner to join a group. Camera frames are processed on the device and are not stored by Offbeat.</p></article><article><h2>Are check-ins based on GPS?</h2><p>No. Offbeat check-ins are stages or text labels you choose manually. The app does not request device location permission.</p></article><article><h2>How do I clear private local data?</h2><p>Use Log out in the app. This removes private group and authentication state from the local Offbeat database while retaining public festival data for offline browsing.</p></article></div></section>`,
	);
}

function privacyPage() {
	return layout(
		"privacy",
		"Offbeat Privacy Policy",
		"How Offbeat handles passkey credentials, festival activity, group data and local storage.",
		`<header class="shell page-head"><div class="eyebrow">Privacy</div><h1>Privacy, without the fog.</h1><p class="lead">Effective 14 August 2026. Offbeat is local-first and does not use advertising or cross-app tracking.</p></header><article class="content"><h2>What Offbeat handles</h2><ul><li><strong>Authentication identifiers:</strong> a pseudonymous public identity and passkey credential data used to authenticate the app. Offbeat does not receive your biometric data or device PIN.</li><li><strong>Profile and activity you provide:</strong> a display name, starred sets, manual stage or meeting-point check-ins, group membership, pins and messages.</li><li><strong>Festival data:</strong> saved festivals, lineup state and synchronisation metadata needed to keep devices consistent.</li><li><strong>Technical data:</strong> peer and protocol identifiers needed to route and verify synchronisation messages.</li></ul><h2>How data is used</h2><p>Data is used only to provide authentication, festival discovery, schedules, groups, chat, offline storage and synchronisation. Offbeat does not sell personal data, serve behavioural advertising or track activity across other companies’ apps and websites.</p><h2>Local-first and peer-to-peer processing</h2><p>Festival and social state is stored in the app’s local database. When synchronisation is enabled, data can be sent directly to peers or relayed through Offbeat’s Cloudflare service. Public festival messages can be replicated to other participants. Private group payloads are encrypted before relay; the relay does not receive the group key.</p><h2>Camera, nearby devices and location</h2><p>QR camera frames are processed on the device and are not retained. Bluetooth and local-network capabilities are used to discover or communicate with nearby peers when available. Offbeat does not request GPS or device location permission; check-ins are labels selected or entered by you.</p><h2>Retention and control</h2><p>Local private data remains until you leave a group, log out, clear the app’s data or uninstall it. Authentication credentials and relayed synchronisation records can remain on Offbeat infrastructure for service operation and abuse prevention. Public messages and data already replicated to peers cannot always be recalled from every device.</p><h2>Service providers</h2><p>Offbeat uses Cloudflare for API, relay and storage infrastructure, Apple and Google platform services for passkeys and app distribution, and public festival-data sources when you request an import.</p><h2>Children</h2><p>Offbeat is intended for festival attendees and is not directed to children under 13.</p><h2>Questions or requests</h2><p>Use the <a href="/support">support page</a> or <a href="${supportUrl}">open a GitHub issue</a>. Do not post passkeys, group keys or other secrets in a public issue.</p><h2>Changes</h2><p>Material changes will be published on this page with a revised effective date.</p></article>`,
	);
}

const pages: Record<SitePage, () => string> = {
	home: homePage,
	support: supportPage,
	privacy: privacyPage,
};

export function renderSitePage(page: SitePage) {
	return pages[page]();
}
