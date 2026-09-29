# TAILOR 0.2.9: distribution and offline activation

The Windows installer is built by the **Windows installer** GitHub Actions workflow and published under the repository's **Releases** page as `TAILOR_0.2.9_x64-setup.exe`. It can also be built on a configured Windows developer PC with `npm.cmd run tauri build`. The local NSIS setup program appears in `src-tauri/target/release/bundle/nsis/`. The installer includes the WebView2 offline installer, so the recipient does not need development tools or an internet connection to install.

Version 0.2.9 counts invoice debt when the paid amount or discount is empty. The dashboard refreshes when opened, on focus, and while visible; direct moves to a completed stage record the tailoring date. Financial reports accept inclusive custom **from** and **to** dates, with daily, monthly, and yearly shortcuts. The report preview and printed sheet show both dates.

New thobes start with yards as their input unit; previously saved thobes keep the unit used when saved. Inventory deduction remains in meters. When the required fabric exceeds available stock, the operator must explicitly acknowledge that the full deduction may result in a negative balance before saving or printing. Missing tailor selections are shown in a centered warning.

Give customers **only the NSIS setup file**. Do not copy the full `H:\TAILOR` development folder or its debug builds to a customer device.

## Issue a license on the owner's computer

1. Keep `TAILOR-owner-private.pem` on **your own computer**. It is not part of this repository, the installer, or a customer's USB drive. Back it up securely: losing it prevents issuing new licenses for this version.
2. Install TAILOR on the recipient's Windows computer and copy the **device code** displayed on the activation screen.
3. On your own computer with Node.js, issue a signed license. Choose its exact expiration date **and hour** in your local timezone (example: Saudi Arabia is `+03:00`):

   ```powershell
   node .\scripts\license-admin.mjs issue "C:\path\only-you-can-access\TAILOR-owner-private.pem" "DEVICE-CODE-FROM-APP" "C:\path\you-control\customer.license.json" --expires "2026-10-04T20:00+03:00"
   ```

   Or, for permanent activation, replace the `--expires` option with `--permanent`. An expiration is an absolute date and time, not a duration counted from when the recipient imports the file. Issue a new file with a later date, or with `--permanent`, to renew the same device. Each new file needs a new filename because the issuer refuses to overwrite an existing file.

4. Bring only `customer.license.json` to the recipient. First select **حتى تاريخ وساعة محددين** and the date/time from the file, or select **للأبد** if the file is permanent. Then use **استيراد ملف التفعيل** and click **تفعيل**. The choice must match the signed file. The app opens its normal setup screen for a fresh shop.

The license and protected activation history are stored separately from `tailor.sqlite` in the current Windows user's application data. Deleting the downloaded license JSON from Downloads after a successful activation does not remove the stored activation. A timed license is checked while the app is open, and the recipient is returned to activation after it expires. The history records that this is not the first activation and that a timed license expired. Existing version 1 licenses were signed without an expiration and remain permanent; do not use an old permanent file for a new trial customer. Copying the installer, the customer database, and the license to another computer will not create a valid activation there. Reinstalling Windows or changing Windows users may require a new activation. Do not send the private key or `license-admin.mjs` to recipients. The offline design cannot report whether a recipient has activated the app; check with the recipient directly.

Development builds (`npm.cmd run tauri dev`) skip licensing to allow local testing. Release builds require a valid signed license on Windows. Timing is enforced against the device clock plus a protected record of the last time the app ran and monotonic time while it is open. An offline computer cannot provide a tamper-proof clock; an administrator could still manipulate its clock or restore old system data. A device that never connects to the internet cannot notify the owner of unauthorized copying.
