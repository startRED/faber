// macOS builds are not notarized, but Apple Silicon still refuses to launch an app without a valid
// signature. Packing edits the Electron bundle and breaks its signature, so re-sign it ad hoc.
const { execFileSync } = require('node:child_process');
const path = require('node:path');

exports.default = async function afterPack(context) {
  if (context.electronPlatformName !== 'darwin') return;
  const appPath = path.join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`);
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', appPath], { stdio: 'inherit', maxBuffer: 16 * 1024 * 1024 });
  execFileSync('codesign', ['--verify', '--deep', '--strict', appPath], { stdio: 'inherit', maxBuffer: 16 * 1024 * 1024 });
};
