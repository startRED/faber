// How electron-builder packs Faber (R459), run by scripts/electron-dist.mjs at build time only. The identity is the Tauri
// Faber's (app id, `faber.exe`, per-user install), so an installed Tauri Faber updates itself into this one in place:
// its updater runs the new installer with `/D=<its folder>` and reopens `<folder>\faber.exe`. The fuses turn off the
// ways to run the app as plain Node or to inspect it, and the app loads only from its checked `app.asar`.
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const pkg = require('../package.json');
// scripts/electron-dist.mjs builds the command line only when the workspace has it.
const cli = path.join(root, 'out/electron-bin/x64/faber-cli.exe');

module.exports = {
  appId: 'io.github.startred.faber', productName: 'Faber', executableName: 'faber',
  directories: { app: path.join(root, 'out/app'), output: path.join(root, 'out/installer') },
  // On macOS both chips are packed on one runner; `arch` is the name (`arm64`, `x64`), and the other chip's Electron is
  // downloaded beside this one's by the release workflow.
  electronDist: process.platform === 'darwin' ? ({ arch }) => {
    return arch === process.arch ? path.join(root, 'node_modules/electron/dist') : path.join(root, `out/electron-dist-${arch}`);
  } : path.join(root, 'node_modules/electron/dist'), electronVersion: pkg.devDependencies.electron,
  asar: true, npmRebuild: false, publish: null, forceCodeSigning: false,
  extraResources: [{ from: path.join(root, 'out/electron-bin/${arch}', process.platform === 'win32' ? 'faber-sidecar.exe' : 'faber-sidecar'), to: process.platform === 'win32' ? 'faber-sidecar.exe' : 'faber-sidecar' }],
  electronFuses: { runAsNode: false, enableNodeOptionsEnvironmentVariable: false, enableNodeCliInspectArguments: false, enableEmbeddedAsarIntegrityValidation: true, onlyLoadAppFromAsar: true, enableCookieEncryption: true, grantFileProtocolExtraPrivileges: false,
    // Flipping a fuse breaks the binary's signature; an unsigned build (`identity: null`) needs the ad-hoc one back, or
    // Apple silicon refuses to start it.
    resetAdHocDarwinSignature: process.platform === 'darwin' },
  win: { icon: path.join(root, 'src-tauri/icons/icon.ico'), target: [{ target: 'nsis', arch: ['x64'] }], extraFiles: require('node:fs').existsSync(cli) ? [{ from: cli, to: 'bin/faber.exe' }] : [] },
  nsis: { oneClick: true, perMachine: false, allowElevation: false, deleteAppDataOnUninstall: false, artifactName: 'Faber_${version}_x64-setup.exe', shortcutName: 'Faber', include: path.join(root, 'installer/windows/faber.nsh') },
  mac: { icon: path.join(root, 'src-tauri/icons/icon.icns'), category: 'public.app-category.productivity', identity: null, target: [{ target: 'dmg', arch: ['arm64', 'x64'] }, { target: 'zip', arch: ['arm64', 'x64'] }] },
  // The updater binds a release to its signed file name (`_<version>_`), so Linux files carry the version in that shape too.
  linux: { icon: path.join(root, 'src-tauri/icons/icon.png'), target: ['AppImage', 'deb'], artifactName: 'Faber_${version}_${arch}.${ext}' },
};
