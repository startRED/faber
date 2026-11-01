// What the browser pane's views may load and be asked, apart from Electron so a plain test can run it. The same rules live in
// Rust (`allowed` in host/src/pane/rules.rs, the DevTools calls in host/src/pane/*.rs); `scripts/pane-rules.test.mjs` keeps
// this copy to the same answers.

/** Whether a view may load `address`: a web page, never one of Faber's own origins (`*.localhost`). */
export function allowedUrl(address: string): boolean {
  let url: URL;
  try {
    url = new URL(address);
  } catch {
    return false;
  }
  const host = url.hostname.replace(/\.+$/, '').toLowerCase();
  return (url.protocol === 'http:' || url.protocol === 'https:') && host !== '' && !host.endsWith('.localhost');
}

/** Requests from pane pages cannot reach Faber's own origins or privileged schemes. */
export function allowedRequest(address: string): boolean {
  try {
    const url = new URL(address);
    if (url.protocol === 'data:' || url.protocol === 'blob:') return true;
    if (!['http:', 'https:', 'ws:', 'wss:'].includes(url.protocol)) return false;
    const host = url.hostname.replace(/\.+$/, '').toLowerCase();
    return host !== '' && !host.endsWith('.localhost');
  } catch {
    return false;
  }
}

/**
 * The only DevTools Protocol commands the host sends a view (listed from host/src/pane/): anything else is refused here, so
 * a command that could navigate a view past its guards (`Page.navigate`), open a new target or reach the browser never runs.
 */
export const CALLS: ReadonlySet<string> = new Set([
  'Emulation.clearDeviceMetricsOverride',
  'Emulation.setDeviceMetricsOverride',
  'Emulation.setTouchEmulationEnabled',
  'Network.setCookie',
  'Network.setCookies',
  'Page.addScriptToEvaluateOnNewDocument',
  'Page.captureScreenshot',
  'Page.createIsolatedWorld',
  'Page.getFrameTree',
  'Page.getNavigationHistory',
  'Page.navigateToHistoryEntry',
  'Page.reload',
  'Page.removeScriptToEvaluateOnNewDocument',
  'Page.resetNavigationHistory',
  'Runtime.evaluate',
]);

/** A profile name the host gives: letters, digits, dots, dashes and underscores, never `persist:` (which would reach the disk). */
export function plainProfile(name: unknown): name is string {
  return typeof name === 'string' && /^[A-Za-z0-9._-]{1,100}$/.test(name);
}
