# TAILOR 0.2.2: distribution and offline activation

The Windows installer is built by the **Windows installer** GitHub Actions workflow and published under the repository's **Releases** page as `TAILOR_0.2.2_x64-setup.exe`. It can also be built on a configured Windows developer PC with `npm.cmd run tauri build`. The local NSIS setup program appears in `src-tauri/target/release/bundle/nsis/`. The installer includes the WebView2 offline installer, so the recipient does not need development tools or an internet connection to install.

Give customers **only the NSIS setup file**. Do not copy the full `H:\TAILOR` development folder or its debug builds to a customer device.

## Issue a license on the owner's computer

1. Keep `TAILOR-owner-private.pem` on **your own computer**. It is not part of this repository, the installer, or a customer's USB drive. Back it up securely: losing it prevents issuing new licenses for this version.
2. Install TAILOR on the recipient's Windows computer and copy the **device code** displayed on the activation screen.
3. On your own computer with Node.js, run:

   ```powershell
   node .\scripts\license-admin.mjs issue "C:\path\only-you-can-access\TAILOR-owner-private.pem" "DEVICE-CODE-FROM-APP" "C:\path\you-control\customer.license.json"
   ```

4. Bring only `customer.license.json` to the recipient, use **استيراد ملف التفعيل** in the app, and click **تفعيل**. The app opens its normal setup screen for a fresh shop.

The license is stored separately from `tailor.sqlite` in the current Windows user's application data. Deleting the downloaded license JSON from Downloads after a successful activation does not remove the stored activation. Copying the installer, the customer database, and the license to another computer will not create a valid activation there. Reinstalling Windows or changing Windows users may require a new activation. Do not send the private key or `license-admin.mjs` to recipients. The offline design cannot report whether a recipient has activated the app; check with the recipient directly.

Development builds (`npm.cmd run tauri dev`) skip licensing to allow local testing. Release builds require a valid signed license on Windows. This is a practical barrier to ordinary copying, not a guarantee against patching by an attacker with control of their own computer. A device that never connects to the internet cannot notify the owner of unauthorized copying.
