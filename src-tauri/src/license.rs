use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

const PUBLIC_KEY_HEX: &str = include_str!("../license_public_key.txt");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub activated: bool,
    pub device_code: String,
    pub expires_at: Option<u64>,
    pub previously_activated: bool,
    pub trial_expired: bool,
    pub clock_warning: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseDocument {
    version: u8,
    device_code: String,
    #[serde(default)]
    expires_at: Option<u64>,
    #[serde(default)]
    license_id: Option<String>,
    signature: String,
}

#[derive(Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ActivationHistory {
    previously_activated: bool,
    trial_expired: bool,
    last_seen_ms: u64,
    #[serde(default)]
    revoked_signatures: Vec<String>,
}

const CLOCK_TOLERANCE_MS: u64 = 5 * 60 * 1000;
const DEACTIVATION_CODE: &str = "ADMIN2";
static LICENSE_LOCK: Mutex<()> = Mutex::new(());
static TIME_ANCHOR: OnceLock<Mutex<Option<(u64, Instant)>>> = OnceLock::new();

fn license_folder(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map_err(|e| e.to_string())
}

fn key_bytes() -> Result<[u8; 32], String> {
    let text = PUBLIC_KEY_HEX.trim();
    if text.len() != 64 {
        return Err("مفتاح التحقق من التفعيل غير صالح".into());
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| "مفتاح التحقق من التفعيل غير صالح")?;
    }
    Ok(bytes)
}

fn verify_license_with_key(
    text: &str,
    device_code: &str,
    public_key: &[u8; 32],
) -> Result<LicenseDocument, String> {
    let license: LicenseDocument =
        serde_json::from_str(text).map_err(|_| "ملف التفعيل غير صالح".to_string())?;
    if license.device_code != device_code {
        return Err("هذا التفعيل مخصص لجهاز آخر".into());
    }
    let payload = match license.version {
        1 if license.expires_at.is_none() => format!("TAILOR-LICENSE-v1:{device_code}"),
        2 => format!(
            "TAILOR-LICENSE-v2:{device_code}:{}",
            license
                .expires_at
                .map_or_else(|| "permanent".into(), |expiry| expiry.to_string())
        ),
        3 => {
            let id = license.license_id.as_deref().ok_or("معرّف ملف التفعيل مفقود")?;
            if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
                return Err("معرّف ملف التفعيل غير صالح".into());
            }
            format!(
                "TAILOR-LICENSE-v3:{device_code}:{}:{id}",
                license.expires_at.map_or_else(|| "permanent".into(), |expiry| expiry.to_string())
            )
        }
        _ => return Err("إصدار ملف التفعيل غير صالح".into()),
    };
    let bytes = STANDARD
        .decode(&license.signature)
        .map_err(|_| "توقيع التفعيل غير صالح".to_string())?;
    let signature =
        Signature::from_slice(&bytes).map_err(|_| "توقيع التفعيل غير صالح".to_string())?;
    let verifier = VerifyingKey::from_bytes(public_key)
        .map_err(|_| "مفتاح التحقق من التفعيل غير صالح".to_string())?;
    verifier
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| "توقيع التفعيل غير صحيح".to_string())?;
    Ok(license)
}

#[cfg(windows)]
fn protect_text(text: &str) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: text.len() as u32,
        pbData: text.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "تعذر حماية تعريف الجهاز: {}",
            std::io::Error::last_os_error()
        ));
    }
    let data =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(data)
}

#[cfg(windows)]
fn unprotect_text(bytes: &[u8]) -> Result<String, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(
            "تعذر قراءة تعريف الجهاز. قد تكون ملفات التفعيل من جهاز آخر؛ اطلب تفعيلًا جديدًا.".into(),
        );
    }
    let data =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    String::from_utf8(data).map_err(|_| "البيانات المحمية غير صالحة".to_string())
}

#[cfg(windows)]
fn device_code(app: &AppHandle) -> Result<String, String> {
    let folder = license_folder(app)?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join("device.dat");
    if path.exists() {
        let code = unprotect_text(&fs::read(path).map_err(|e| e.to_string())?)?;
        uuid::Uuid::parse_str(&code).map_err(|_| "تعريف الجهاز غير صالح".to_string())?;
        return Ok(code);
    }
    let code = uuid::Uuid::new_v4().to_string();
    let encrypted = protect_text(&code)?;
    fs::write(&path, encrypted).map_err(|e| e.to_string())?;
    Ok(code)
}

#[cfg(not(windows))]
fn device_code(_: &AppHandle) -> Result<String, String> {
    Err("النسخة المرخصة تعمل على Windows فقط".into())
}

fn observed_ms() -> Result<u64, String> {
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "ساعة الجهاز غير صحيحة".to_string())?
        .as_millis() as u64;
    let mut anchor = TIME_ANCHOR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "تعذر فحص ساعة الجهاز".to_string())?;
    let instant = Instant::now();
    match anchor.as_mut() {
        Some((base, started)) => {
            let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            let current = base.saturating_add(elapsed).max(wall);
            if wall > base.saturating_add(elapsed) {
                *base = wall;
                *started = instant;
            }
            Ok(current)
        }
        None => {
            *anchor = Some((wall, instant));
            Ok(wall)
        }
    }
}

// The wall clock is checked across launches, and elapsed monotonic time is used while open.
fn evaluate_document(
    document: &LicenseDocument,
    history: &mut ActivationHistory,
    now_ms: u64,
) -> (bool, bool, bool) {
    let clock_warning = history.previously_activated
        && history.last_seen_ms > now_ms.saturating_add(CLOCK_TOLERANCE_MS);
    let effective_ms = now_ms.max(history.last_seen_ms);
    let expired = document
        .expires_at
        .is_some_and(|expiry| effective_ms >= expiry);
    history.previously_activated = true;
    if expired {
        history.trial_expired = true;
    }
    if !clock_warning {
        history.last_seen_ms = effective_ms;
    }
    (!expired && !clock_warning, expired, clock_warning)
}

fn validate_activation_choice(
    document: &LicenseDocument,
    mode: &str,
    selected_expiry_at: Option<u64>,
) -> Result<(), String> {
    let matches = match (mode, document.expires_at) {
        ("permanent", None) => selected_expiry_at.is_none(),
        ("timed", Some(expiry)) => selected_expiry_at == Some(expiry),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err("التاريخ أو نوع التفعيل المختار لا يطابق ملف التفعيل الموقّع. أصدر ملفًا بالتاريخ المطلوب.".into())
    }
}

#[cfg(windows)]
fn read_history(folder: &PathBuf) -> Result<ActivationHistory, String> {
    let path = folder.join("activation-state.dat");
    if !path.exists() {
        return Ok(ActivationHistory::default());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let text = unprotect_text(&bytes)?;
    serde_json::from_str(&text).map_err(|_| "تعذر قراءة سجل التفعيل".to_string())
}

#[cfg(windows)]
fn save_history(folder: &PathBuf, history: &ActivationHistory) -> Result<(), String> {
    let text = serde_json::to_string(history).map_err(|e| e.to_string())?;
    fs::write(folder.join("activation-state.dat"), protect_text(&text)?).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn read_history(_: &PathBuf) -> Result<ActivationHistory, String> {
    Err("النسخة المرخصة تعمل على Windows فقط".into())
}

#[cfg(not(windows))]
fn save_history(_: &PathBuf, _: &ActivationHistory) -> Result<(), String> {
    Err("النسخة المرخصة تعمل على Windows فقط".into())
}

#[cfg(not(debug_assertions))]
fn stored_status(app: &AppHandle) -> Result<LicenseStatus, String> {
    let _guard = LICENSE_LOCK
        .lock()
        .map_err(|_| "تعذر قراءة التفعيل".to_string())?;
    let folder = license_folder(app)?;
    let code = device_code(app)?;
    let mut history = read_history(&folder)?;
    let mut status = LicenseStatus {
        activated: false,
        device_code: code.clone(),
        expires_at: None,
        previously_activated: history.previously_activated,
        trial_expired: history.trial_expired,
        clock_warning: false,
    };
    let path = folder.join("license.json");
    if !path.exists() {
        return Ok(status);
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let document = match verify_license_with_key(&text, &code, &key_bytes()?) {
        Ok(document) => document,
        Err(_) => return Ok(status),
    };
    if history.revoked_signatures.contains(&document.signature) {
        return Ok(status);
    }
    let now = observed_ms()?;
    let previous = ActivationHistory {
        previously_activated: history.previously_activated,
        trial_expired: history.trial_expired,
        last_seen_ms: history.last_seen_ms,
        revoked_signatures: history.revoked_signatures.clone(),
    };
    let (active, expired, clock_warning) = evaluate_document(&document, &mut history, now);
    if previous.previously_activated != history.previously_activated
        || previous.trial_expired != history.trial_expired
        || history.last_seen_ms.saturating_sub(previous.last_seen_ms) >= 60_000
    {
        save_history(&folder, &history)?;
    }
    status.activated = active;
    status.expires_at = document.expires_at;
    status.previously_activated = history.previously_activated;
    status.trial_expired = history.trial_expired || expired;
    status.clock_warning = clock_warning;
    Ok(status)
}

pub fn activated(app: &AppHandle) -> Result<bool, String> {
    // Developer builds stay usable while creating and testing the program.
    #[cfg(debug_assertions)]
    {
        let _ = app;
        return Ok(true);
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(stored_status(app)?.activated)
    }
}

#[tauri::command]
pub fn license_status(
    app: AppHandle,
    state: tauri::State<'_, crate::SessionState>,
) -> Result<LicenseStatus, String> {
    #[cfg(debug_assertions)]
    {
        let _ = (app, state);
        return Ok(LicenseStatus {
            activated: true,
            device_code: "DEVELOPMENT".into(),
            expires_at: None,
            previously_activated: false,
            trial_expired: false,
            clock_warning: false,
        });
    }
    #[cfg(not(debug_assertions))]
    {
        let status = stored_status(&app)?;
        if !status.activated {
            let mut guard = state
                .0
                .lock()
                .map_err(|_| "تعذر إنهاء جلسة البرنامج".to_string())?;
            if let Some(id) = guard.take() {
                crate::finish_session(&app, id)?;
            }
        }
        Ok(status)
    }
}

#[tauri::command]
pub fn activate_license(
    app: AppHandle,
    state: tauri::State<'_, crate::SessionState>,
    license_text: String,
    activation_mode: String,
    selected_expiry_at: Option<u64>,
) -> Result<LicenseStatus, String> {
    #[cfg(debug_assertions)]
    {
        let _ = (
            app,
            state,
            license_text,
            activation_mode,
            selected_expiry_at,
        );
        return Err("التفعيل متاح في النسخة المثبتة فقط".into());
    }
    #[cfg(not(debug_assertions))]
    {
        if license_text.len() > 4096 {
            return Err("ملف التفعيل كبير جدًا".into());
        }
        let _guard = LICENSE_LOCK
            .lock()
            .map_err(|_| "تعذر حفظ التفعيل".to_string())?;
        let code = device_code(&app)?;
        let document = verify_license_with_key(&license_text, &code, &key_bytes()?)?;
        validate_activation_choice(&document, &activation_mode, selected_expiry_at)?;
        let folder = license_folder(&app)?;
        let mut history = read_history(&folder)?;
        if history.revoked_signatures.contains(&document.signature) {
            return Err("أُلغي ملف التفعيل هذا سابقًا على هذا الجهاز. اطلب ملف تفعيل جديدًا.".into());
        }
        let now = observed_ms()?;
        let (valid, expired, clock_warning) = evaluate_document(&document, &mut history, now);
        if expired {
            return Err("انتهت صلاحية ملف التفعيل هذا؛ اطلب ملفًا بتاريخ جديد.".into());
        }
        if clock_warning {
            return Err(
                "ساعة الجهاز متأخرة عن آخر استخدام مسجل. صحح تاريخ الجهاز وساعته ثم حاول ثانية."
                    .into(),
            );
        }
        if !valid {
            return Err("تعذر التحقق من مدة التفعيل".into());
        }
        save_history(&folder, &history)?;
        fs::write(folder.join("license.json"), license_text).map_err(|e| e.to_string())?;
        drop(_guard);
        let mut guard = state
            .0
            .lock()
            .map_err(|_| "تعذر بدء جلسة البرنامج".to_string())?;
        if guard.is_none() {
            *guard = Some(crate::begin_session(&app)?);
        }
        Ok(LicenseStatus {
            activated: true,
            device_code: code,
            expires_at: document.expires_at,
            previously_activated: true,
            trial_expired: history.trial_expired,
            clock_warning: false,
        })
    }
}

#[tauri::command]
pub fn deactivate_license(
    app: AppHandle,
    state: tauri::State<'_, crate::SessionState>,
    admin_code: String,
) -> Result<LicenseStatus, String> {
    #[cfg(debug_assertions)]
    {
        let _ = (app, state, admin_code);
        return Err("إلغاء التفعيل متاح في النسخة المثبتة فقط".into());
    }
    #[cfg(not(debug_assertions))]
    {
        if admin_code != DEACTIVATION_CODE {
            return Err("رمز إلغاء التفعيل غير صحيح".into());
        }
        let guard = LICENSE_LOCK.lock().map_err(|_| "تعذر إلغاء التفعيل".to_string())?;
        let folder = license_folder(&app)?;
        let path = folder.join("license.json");
        let text = fs::read_to_string(&path).map_err(|_| "لا يوجد ملف تفعيل لإلغائه".to_string())?;
        let document = verify_license_with_key(&text, &device_code(&app)?, &key_bytes()?)?;
        let mut history = read_history(&folder)?;
        if !history.revoked_signatures.contains(&document.signature) {
            history.revoked_signatures.push(document.signature);
        }
        history.previously_activated = true;
        save_history(&folder, &history)?;
        fs::remove_file(path).map_err(|e| format!("سُجل إلغاء التفعيل، لكن تعذر إزالة الملف: {e}"))?;
        drop(guard);
        license_status(app, state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn license_must_match_device_and_signature() {
        let signer = SigningKey::from_bytes(&[7u8; 32]);
        let code = "42e3b5f7-804e-4aa0-bdcc-97b66dd2f38f";
        let signature = signer.sign(format!("TAILOR-LICENSE-v1:{code}").as_bytes());
        let text = serde_json::json!({"version": 1, "deviceCode": code, "signature": STANDARD.encode(signature.to_bytes())}).to_string();
        assert!(verify_license_with_key(&text, code, signer.verifying_key().as_bytes()).is_ok());
        assert!(verify_license_with_key(
            &text,
            "42e3b5f7-804e-4aa0-bdcc-97b66dd2f380",
            signer.verifying_key().as_bytes()
        )
        .is_err());
        let wrong_version = serde_json::json!({"version": 2, "deviceCode": code, "signature": STANDARD.encode(signature.to_bytes())}).to_string();
        assert!(
            verify_license_with_key(&wrong_version, code, signer.verifying_key().as_bytes())
                .is_err()
        );
        let tampered = text.replace("42e3b5f7", "52e3b5f7");
        assert!(verify_license_with_key(
            &tampered,
            "52e3b5f7-804e-4aa0-bdcc-97b66dd2f38f",
            signer.verifying_key().as_bytes()
        )
        .is_err());
    }

    #[test]
    fn timed_license_expires_at_the_signed_millisecond() {
        let signer = SigningKey::from_bytes(&[9u8; 32]);
        let code = "42e3b5f7-804e-4aa0-bdcc-97b66dd2f38f";
        let expiry = 1_800_000_000_000_u64;
        let payload = format!("TAILOR-LICENSE-v2:{code}:{expiry}");
        let signature = signer.sign(payload.as_bytes());
        let text = serde_json::json!({"version":2,"deviceCode":code,"expiresAt":expiry,"signature":STANDARD.encode(signature.to_bytes())}).to_string();
        let verified =
            verify_license_with_key(&text, code, signer.verifying_key().as_bytes()).unwrap();
        assert!(validate_activation_choice(&verified, "timed", Some(expiry)).is_ok());
        assert!(validate_activation_choice(&verified, "permanent", None).is_err());
        assert!(validate_activation_choice(&verified, "timed", Some(expiry + 60_000)).is_err());
        let mut history = ActivationHistory::default();
        assert_eq!(
            evaluate_document(&verified, &mut history, expiry - 1),
            (true, false, false)
        );
        assert_eq!(
            evaluate_document(&verified, &mut history, expiry),
            (false, true, false)
        );
        assert!(history.previously_activated && history.trial_expired);
        assert!(verify_license_with_key(
            &text.replace(&expiry.to_string(), &(expiry + 60_000).to_string()),
            code,
            signer.verifying_key().as_bytes()
        )
        .is_err());
    }

    #[test]
    fn permanent_renewal_does_not_erase_trial_history() {
        let signer = SigningKey::from_bytes(&[10u8; 32]);
        let code = "42e3b5f7-804e-4aa0-bdcc-97b66dd2f38f";
        let signature = signer.sign(format!("TAILOR-LICENSE-v2:{code}:permanent").as_bytes());
        let text = serde_json::json!({"version":2,"deviceCode":code,"expiresAt":null,"signature":STANDARD.encode(signature.to_bytes())}).to_string();
        let verified =
            verify_license_with_key(&text, code, signer.verifying_key().as_bytes()).unwrap();
        assert!(validate_activation_choice(&verified, "permanent", None).is_ok());
        assert!(validate_activation_choice(&verified, "timed", Some(1_900_000_000_000)).is_err());
        let mut history = ActivationHistory {
            previously_activated: true,
            trial_expired: true,
            last_seen_ms: 1_800_000_000_000,
            revoked_signatures: vec![],
        };
        assert_eq!(
            evaluate_document(&verified, &mut history, 1_800_000_000_001),
            (true, false, false)
        );
        assert!(history.trial_expired);
    }

    #[test]
    fn clock_rollback_blocks_offline_use() {
        let document = LicenseDocument {
            version: 2,
            device_code: "device".into(),
            expires_at: Some(1_900_000_000_000),
            license_id: None,
            signature: "".into(),
        };
        let mut history = ActivationHistory {
            previously_activated: true,
            trial_expired: false,
            last_seen_ms: 1_800_000_000_000,
            revoked_signatures: vec![],
        };
        assert_eq!(
            evaluate_document(
                &document,
                &mut history,
                1_800_000_000_000 - CLOCK_TOLERANCE_MS - 1
            ),
            (false, false, true)
        );
    }

    #[test]
    fn old_activation_history_loads_without_revocations() {
        let history: ActivationHistory = serde_json::from_str(
            r#"{"previouslyActivated":true,"trialExpired":false,"lastSeenMs":1234}"#,
        )
        .unwrap();
        assert!(history.previously_activated);
        assert!(history.revoked_signatures.is_empty());
        let mut history = history;
        history.revoked_signatures.push("old-license".into());
        let saved = serde_json::to_string(&history).unwrap();
        let loaded: ActivationHistory = serde_json::from_str(&saved).unwrap();
        assert!(loaded.revoked_signatures.contains(&"old-license".to_string()));
        assert!(!loaded.revoked_signatures.contains(&"new-license".to_string()));
    }

    #[test]
    fn new_permanent_license_can_replace_revoked_one() {
        let signer = SigningKey::from_bytes(&[11u8; 32]);
        let code = "42e3b5f7-804e-4aa0-bdcc-97b66dd2f38f";
        let make = |id: &str| {
            let payload = format!("TAILOR-LICENSE-v3:{code}:permanent:{id}");
            let signature = signer.sign(payload.as_bytes());
            serde_json::json!({"version":3,"deviceCode":code,"expiresAt":null,"licenseId":id,"signature":STANDARD.encode(signature.to_bytes())}).to_string()
        };
        let old = verify_license_with_key(&make("00000000000000000000000000000001"), code, signer.verifying_key().as_bytes()).unwrap();
        let new = verify_license_with_key(&make("00000000000000000000000000000002"), code, signer.verifying_key().as_bytes()).unwrap();
        assert_ne!(old.signature, new.signature);
        let history = ActivationHistory { revoked_signatures: vec![old.signature.clone()], ..Default::default() };
        assert!(history.revoked_signatures.contains(&old.signature));
        assert!(!history.revoked_signatures.contains(&new.signature));
        let tampered = make("00000000000000000000000000000002").replace("00000000000000000000000000000002", "00000000000000000000000000000003");
        assert!(verify_license_with_key(&tampered, code, signer.verifying_key().as_bytes()).is_err());
    }
}
