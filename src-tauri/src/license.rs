use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

const PUBLIC_KEY_HEX: &str = include_str!("../license_public_key.txt");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub activated: bool,
    pub device_code: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseDocument {
    version: u8,
    device_code: String,
    signature: String,
}

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
) -> Result<(), String> {
    let license: LicenseDocument =
        serde_json::from_str(text).map_err(|_| "ملف التفعيل غير صالح".to_string())?;
    if license.version != 1 || license.device_code != device_code {
        return Err("هذا التفعيل مخصص لجهاز آخر".into());
    }
    let bytes = STANDARD
        .decode(&license.signature)
        .map_err(|_| "توقيع التفعيل غير صالح".to_string())?;
    let signature =
        Signature::from_slice(&bytes).map_err(|_| "توقيع التفعيل غير صالح".to_string())?;
    let verifier = VerifyingKey::from_bytes(public_key)
        .map_err(|_| "مفتاح التحقق من التفعيل غير صالح".to_string())?;
    verifier
        .verify(
            format!("TAILOR-LICENSE-v1:{device_code}").as_bytes(),
            &signature,
        )
        .map_err(|_| "توقيع التفعيل غير صحيح".to_string())
}

#[cfg(windows)]
fn protect_device_code(text: &str) -> Result<Vec<u8>, String> {
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
fn unprotect_device_code(bytes: &[u8]) -> Result<String, String> {
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
    let code = String::from_utf8(data).map_err(|_| "تعريف الجهاز غير صالح".to_string())?;
    uuid::Uuid::parse_str(&code).map_err(|_| "تعريف الجهاز غير صالح".to_string())?;
    Ok(code)
}

#[cfg(windows)]
fn device_code(app: &AppHandle) -> Result<String, String> {
    let folder = license_folder(app)?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join("device.dat");
    if path.exists() {
        return unprotect_device_code(&fs::read(path).map_err(|e| e.to_string())?);
    }
    let code = uuid::Uuid::new_v4().to_string();
    let encrypted = protect_device_code(&code)?;
    fs::write(&path, encrypted).map_err(|e| e.to_string())?;
    Ok(code)
}

#[cfg(not(windows))]
fn device_code(_: &AppHandle) -> Result<String, String> {
    Err("النسخة المرخصة تعمل على Windows فقط".into())
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
        let folder = license_folder(app)?;
        let code = device_code(app)?;
        let path = folder.join("license.json");
        if !path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Ok(verify_license_with_key(&content, &code, &key_bytes()?).is_ok())
    }
}

#[tauri::command]
pub fn license_status(app: AppHandle) -> Result<LicenseStatus, String> {
    #[cfg(debug_assertions)]
    {
        return Ok(LicenseStatus {
            activated: true,
            device_code: "DEVELOPMENT".into(),
        });
    }
    #[cfg(not(debug_assertions))]
    {
        let device_code = device_code(&app)?;
        Ok(LicenseStatus {
            activated: activated(&app)?,
            device_code,
        })
    }
}

#[tauri::command]
pub fn activate_license(
    app: AppHandle,
    state: tauri::State<'_, crate::SessionState>,
    license_text: String,
) -> Result<LicenseStatus, String> {
    #[cfg(debug_assertions)]
    {
        let _ = (app, state, license_text);
        return Err("التفعيل متاح في النسخة المثبتة فقط".into());
    }
    #[cfg(not(debug_assertions))]
    {
        if license_text.len() > 4096 {
            return Err("ملف التفعيل كبير جدًا".into());
        }
        let code = device_code(&app)?;
        verify_license_with_key(&license_text, &code, &key_bytes()?)?;
        fs::write(license_folder(&app)?.join("license.json"), license_text)
            .map_err(|e| e.to_string())?;
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
        })
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
}
