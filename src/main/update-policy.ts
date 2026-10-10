export type UpdateMode = 'install' | 'notify';

// macOS builds are not notarized, and electron-updater cannot install unsigned updates there,
// so on macOS Faber only tells the user a new version exists and opens the download page.
export function updateMode(platform: NodeJS.Platform): UpdateMode {
  return platform === 'darwin' ? 'notify' : 'install';
}
