// The page in Faber's browser sits in a rounded card of the screen, but the pane is a native rectangle: this draws its four
// corners over the page, smooth, in the colour of the card around it, and gives the page's scrollbars the screen's thin
// rounded thumb with no arrows. It runs in a world of Faber's own on every document (`round` in mod.rs), so the page's
// scripts never see it; the corners take no clicks, no focus and no words. __ID__, __R__ and __C__ are what the shell
// writes in: the element's random name (hex), the radius in pixels and the colour as "r, g, b", numbers only.
(() => {
  const id = '__ID__';
  const r = __R__;
  const color = 'rgb(__C__)';
  // A stylesheet built in script, never a <style> element, so a page that refuses inline styles still takes it. Once per
  // document: this world's globals live as long as the document does.
  if (!globalThis.faberThinBars) {
    globalThis.faberThinBars = true;
    try {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(
        '::-webkit-scrollbar{width:12px;height:12px}' +
          '::-webkit-scrollbar-track,::-webkit-scrollbar-corner{background:transparent}' +
          '::-webkit-scrollbar-button{display:none;width:0;height:0}' +
          '::-webkit-scrollbar-thumb{min-height:40px;border:4px solid transparent;border-radius:999px;background-color:rgba(128,128,128,.45);background-clip:padding-box}' +
          '::-webkit-scrollbar-thumb:hover,::-webkit-scrollbar-thumb:active{border-width:3px;background-color:rgba(128,128,128,.7)}',
      );
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
    } catch {
      // An engine with no built stylesheets keeps the page's own scrollbars.
    }
  }
  document.getElementById(id)?.remove();
  // Only the page's own document sits in the card: a frame inside it keeps its corners, and the agent's pictures of it too.
  if (!(r > 0) || window !== window.top) return;
  const put = (el, css) => {
    for (const [name, value] of Object.entries(css)) el.style.setProperty(name, value, 'important');
  };
  const host = document.createElement('div');
  host.id = id;
  host.setAttribute('aria-hidden', 'true');
  put(host, {
    position: 'fixed',
    inset: '0',
    margin: '0',
    padding: '0',
    border: '0',
    background: 'none',
    display: 'block',
    contain: 'strict',
    'pointer-events': 'none',
    'z-index': '2147483647',
  });
  const shadow = host.attachShadow({ mode: 'closed' });
  for (const [right, bottom] of [[0, 0], [1, 0], [0, 1], [1, 1]]) {
    const corner = document.createElement('div');
    // The arc's centre is the corner's inner point; outside it, the card's colour, blended over under a pixel.
    put(corner, {
      position: 'absolute',
      width: `${r}px`,
      height: `${r}px`,
      [right ? 'right' : 'left']: '0',
      [bottom ? 'bottom' : 'top']: '0',
      background: `radial-gradient(circle at ${right ? 0 : 100}% ${bottom ? 0 : 100}%, transparent ${r - 0.75}px, ${color} ${r}px)`,
    });
    shadow.append(corner);
  }
  // Before the page has parsed its first tag there is no <html> yet: the corners go in as soon as there is.
  const attach = () => {
    if (!document.documentElement) return false;
    document.documentElement.append(host);
    return true;
  };
  if (!attach()) new MutationObserver((_, seen) => attach() && seen.disconnect()).observe(document, { childList: true });
})();
