// The webview Faber runs in reads woff2, so the older woff copies that Fontsource lists after it are never used and never bundled.
// A PostCSS plugin (vite.config.ts) removes them from every @font-face `src` before the build looks for the files.

/** `src` of a font face without its woff fallback: `url(a.woff2) format('woff2'), url(a.woff) format('woff')` -> the first. */
export function woff2Only(src) {
  return src.replace(/,\s*url\([^)]*\.woff\)\s*format\((['"])woff\1\)/g, '');
}

/** The PostCSS plugin: it rewrites the `src` of every @font-face. */
export function woff2OnlyPlugin() {
  return {
    postcssPlugin: 'faber-woff2-only',
    Declaration: {
      src(declaration) {
        if (declaration.parent?.type !== 'atrule' || declaration.parent.name !== 'font-face') return;
        const next = woff2Only(declaration.value);
        if (next !== declaration.value) declaration.value = next;
      },
    },
  };
}
woff2OnlyPlugin.postcss = true;
