# TAILOR 0.3.7: distribution and offline activation

Build and publish installers on the owner's Windows computer with `node scripts/build-installer.mjs --publish`; see [RELEASING.md](RELEASING.md) for the local workflow, tests, version checks, and the no-GitHub-CLI option. GitHub Actions can also publish to **Releases** automatically after account billing is restored. The local NSIS setup appears in `src-tauri/target/release/bundle/nsis/` as `TAILOR_0.3.7_x64-setup.exe` for this version. The installer includes the WebView2 offline installer, so recipients do not need development tools or internet to install.

Version 0.2.9 counts invoice debt when the paid amount or discount is empty. The dashboard refreshes when opened, on focus, and while visible; direct moves to a completed stage record the tailoring date. Financial reports accept inclusive custom **from** and **to** dates, with daily, monthly, and yearly shortcuts. The report preview and printed sheet show both dates.

Version 0.3.0 separates white and colored fabric in stock and tailoring, records catalog and color numbers for colored cloth, and shows total supply value before saving. Priced supply requires a supplier so its purchase value enters the supplier debt ledger. Existing fabrics are retained as white by the migration. Ready production consumes cloth and creates ready stock; ready sales decrement that stock. Copy `tailor.sqlite` to a safe backup before the first launch of this version. Existing app identity and license verification key remain unchanged.

New thobes start with yards as their input unit; previously saved thobes keep the unit used when saved. Inventory deduction remains in meters. When the required fabric exceeds available stock, the operator must explicitly acknowledge that the full deduction may result in a negative balance before saving or printing. Missing tailor selections are shown in a centered warning.

Give customers **only the NSIS setup file**. Do not copy the full `G:\TAILOR-0.3.7` development folder or its debug builds to a customer device.

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

Version 0.3.2 starts maximized while retaining window controls, corrects dashboard grid rows, supports split cash/network/transfer payments for initial invoices and later debt settlements with individual ledger entries, and advances Enter through ready-stock measurements in order.

Version 0.3.4 adds a button on the duplicate mobile warning to open that customer's record and measurements on demand, and makes the fabric and split payment controls smaller in the tailoring form. The combined laundry and measurement print button and duplicate mobile validation from 0.3.3 remain available.

Version 0.3.5 adds a saved minimum invoice price, transactional customer creation with the first invoice, one-step customer deletion with linked invoice cleanup, a compact ready-garment design picker, and optional scheduled daily report printing when TAILOR is open. Windows printer status depends on the driver and must be tested with the actual printer.

Version 0.3.6 keeps the small price control unlabeled in the tailoring form and shows a prominent audible warning on a rejected low price. Choosing a fabric group without its color now produces a specific warning. The dashboard loads delivered orders from the current local day only; older records remain in customer history and financial reports. Settings can deactivate a signed license using the owner code. The exact revoked license is rejected again on that Windows user profile; a newly issued signed license can be used. New licenses use a unique signed issuance ID (version 3), so a fresh permanent license for the same device can work after cancellation; older version 1 and 2 files still verify. Deactivation keeps customer data and the device identity. The common code is embedded in the installed program and visible in the repository, so it is an operational control, not a secret against someone able to inspect or modify the program.

Version 0.3.7 keeps the dashboard reachable when its warning banner or a short screen adds vertical content. Moving a ready order to delivered now opens an invoice-specific payment dialog before changing status. It shows the current order invoice debt and the customer total, accepts cash/network/transfer amounts per invoice for current and older debts, and commits all payments with the delivery status in one transaction. It can also deliver without a payment while preserving debt. Bulk transfer to delivered is disabled to enforce this accounting step. When one invoice contains multiple garments, its debt is tracked for the invoice as a whole.
