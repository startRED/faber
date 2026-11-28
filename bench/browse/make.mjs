// Writes the fixture pages the browser bench reads (`tests/ui/browse-bench.check.ts`): `node bench/browse/make.mjs`.
// They stand for what agents meet: a shop (many look-alike buttons), a long docs page, a short form flow and a page with
// secrets on it. Plain HTML so the check builds each in the screen's own browser; nothing here runs a server or a process.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const out = dirname(fileURLToPath(import.meta.url));
mkdirSync(out, { recursive: true });
const put = (name, html) => writeFileSync(join(out, name), html);

const names = ['Linen shirt', 'Canvas tote', 'Wool scarf', 'Leather belt', 'Trail runner', 'Rain jacket', 'Denim jeans', 'Cotton socks', 'Beanie hat', 'Silk tie', 'Hiking boots', 'Fleece vest', 'Chino pants', 'Sun hat', 'Down coat', 'Wallet', 'Sunglasses', 'Gloves', 'Polo shirt', 'Cardigan', 'Sandals', 'Backpack', 'Watch strap', 'Bow tie'];
const cards = names
  .map(
    (n, i) => `
    <article class="card" id="p${i + 1}">
      <h3>${n}</h3>
      <p class="blurb">A well made ${n.toLowerCase()} in a relaxed fit, available in four colours and all sizes.</p>
      <p class="price">$${19 + i * 3}.00 <span class="stock">${i % 5 === 0 ? 'Only 2 left' : 'In stock'}</span></p>
      <a href="#d${i + 1}">Details</a>
      <button type="button" data-n="${n}">Add to cart</button>
    </article>`,
  )
  .join('');
const nav = ['Home', 'New in', 'Women', 'Men', 'Kids', 'Sale', 'Journal', 'Stores'].map((l) => `<a href="#${l.toLowerCase().replace(' ', '-')}">${l}</a>`).join(' ');
const footer = ['Shipping', 'Returns', 'Sizing guide', 'Gift cards', 'Careers', 'Press', 'Privacy', 'Terms', 'Contact us', 'Help centre', 'Order status', 'Store finder', 'Sustainability', 'Accessibility', 'Cookie settings', 'Affiliates']
  .map((l, i) => `<li><a href="#f${i}">${l}</a></li>`)
  .join('');
put(
  'shop.html',
  `<!doctype html><html lang="en"><body>
<header><nav aria-label="Main">${nav}</nav>
  <form role="search"><label>Search the shop <input name="q" placeholder="Search"></label><button type="button">Search</button></form>
  <a href="#cart" aria-label="Cart, 0 items">Cart (<span id="count">0</span>)</a></header>
<main><h1>Spring collection</h1>
<aside aria-label="Filters"><h2>Filters</h2>
  <label><input type="checkbox" name="in-stock"> In stock only</label>
  <label><input type="checkbox" name="sale"> On sale</label>
  <label><input type="checkbox" name="new"> New arrivals</label>
  <label>Sort by <select name="sort"><option>Featured</option><option>Price: low to high</option><option>Price: high to low</option></select></label>
  <label>Size <select name="size"><option>S</option><option>M</option><option>L</option></select></label>
  <button type="button">Apply filters</button>
</aside>
<section aria-label="Products">${cards}</section></main>
<footer><ul>${footer}</ul><p>© 2026 Linen &amp; Co. All rights reserved. Prices include tax where applicable.</p></footer>
<script>
 const root = document.currentScript.parentNode; let n = 0;
 root.addEventListener('click', (e) => {
   const b = e.target.closest('button[data-n]'); if (!b) return;
   n += 1; root.querySelector('#count').textContent = n;
   let t = root.querySelector('#toast');
   if (!t) { t = document.createElement('div'); t.id = 'toast'; t.setAttribute('role', 'status'); root.append(t); }
   t.textContent = 'Added ' + b.dataset.n + ' to your cart.';
 });
</script></body></html>`,
);

const para = (i) =>
  `<p>Section ${i} explains one part of the tool in detail. The configuration lives in a single file at the root of the project, and every option has a default that works for most teams. Changing an option takes effect the next time the tool starts; nothing needs to be rebuilt. When two options disagree, the more specific one wins, and the tool prints which rule it used so the choice can be traced later on.</p><p>For example, setting <code>retries</code> to 3 makes a failed request run up to three more times, waiting a little longer between each try. The wait grows by a factor you can set with <code>backoff</code>. Both values must be whole numbers; anything else is reported at start-up with the line it came from.</p>`;
const side = ['Introduction', 'Install', 'Quick start', 'Configuration', 'Options', 'Retries', 'Timeouts', 'Caching', 'Plugins', 'Hooks', 'Events', 'Logging', 'Errors', 'Testing', 'Deploying', 'Upgrading', 'Migration', 'Security', 'Performance', 'Recipes', 'Glossary', 'FAQ', 'Changelog', 'Roadmap', 'Community', 'Contributing', 'License', 'Credits', 'Support', 'Contact']
  .map((l, i) => `<li><a href="#s${i}">${l}</a></li>`)
  .join('');
put(
  'article.html',
  `<!doctype html><html lang="en"><body>
<header><a href="#home">Toolkit docs</a> <label>Search the docs <input name="q" placeholder="Search docs"></label> <button type="button" aria-label="Toggle dark mode">Dark</button></header>
<div><nav aria-label="Sidebar"><ul>${side}</ul></nav>
<main><h1>Configuration</h1>${Array.from({ length: 14 }, (_, i) => `<h2>Part ${i + 1}</h2>${para(i + 1)}`).join('')}
<nav aria-label="Pagination"><a href="#prev">Previous: Quick start</a> <a href="#next">Next: Options</a></nav>
<p>Was this page helpful? <button type="button" id="yes">Yes</button> <button type="button" id="no">No</button></p><p id="thanks" hidden>Thanks for the feedback.</p></main></div>
<script>const root = document.currentScript.parentNode; for (const id of ['yes', 'no']) root.querySelector('#' + id).addEventListener('click', () => { root.querySelector('#thanks').hidden = false; });</script>
</body></html>`,
);

put(
  'signup.html',
  `<!doctype html><html lang="en"><body>
<header><a href="#home">Acme Cloud</a> <nav><a href="#pricing">Pricing</a> <a href="#docs">Docs</a> <a href="#status">Status</a></nav></header>
<main><h1>Create your workspace</h1>
<p>Start a 14 day trial. No card needed. You can invite your team afterwards and change the plan at any time from the billing page.</p>
<form id="f">
  <label>Full name <input id="name" name="name" autocomplete="name"></label>
  <label>Work email <input id="email" name="email" type="email" autocomplete="email"></label>
  <label>Company <input id="company" name="company"></label>
  <fieldset><legend>Plan</legend>
    <label><input type="radio" name="plan" value="free"> Free</label>
    <label><input type="radio" name="plan" value="team"> Team</label>
    <label><input type="radio" name="plan" value="business"> Business</label></fieldset>
  <label><input type="checkbox" id="terms" name="terms"> I accept the terms of service</label>
  <button type="submit" id="go" disabled>Create workspace</button>
  <button type="button" id="cancel">Cancel</button>
</form>
<div id="result" role="status"></div></main>
<footer><a href="#privacy">Privacy</a> <a href="#terms">Terms</a> <a href="#contact">Contact</a></footer>
<script>
 const root = document.currentScript.parentNode, f = root.querySelector('#f'), go = root.querySelector('#go');
 const ready = () => { go.disabled = !(f.name.value && f.email.value && f.terms.checked && f.plan.value); };
 f.addEventListener('input', ready); f.addEventListener('change', ready);
 f.addEventListener('submit', (e) => { e.preventDefault(); root.querySelector('#result').textContent = 'Workspace created for ' + f.email.value + ' on the ' + f.plan.value + ' plan.'; });
</script></body></html>`,
);

put(
  'secrets.html',
  `<!doctype html><html lang="en"><body>
<main><h1>Account settings</h1>
<label>Display name <input id="dn" value="Ana"></label>
<label>API key <input id="key" value="sk-live-4eC39HqLyjWDarjtT1zdp7dc"></label>
<label>Recovery phrase <input id="rec" type="password" value="correct horse battery staple"></label>
<label>Backup number <input id="cc" name="backup" value="4242 4242 4242 4242"></label>
<p>Your token is ghp_16C7e42F292c6912E7710c838347Ae178B4a and your card ends 4242 4242 4242 4242.</p>
<button type="button">Save</button></main></body></html>`,
);
