//! Pro for the apps (Pro+Apps, Team): the signed licence pdf.dk issues (`GET /api/licence/client`,
//! Ed25519, valid 7 days) – its `pro` and nothing else. A plain Pro is website-only and gets
//! `pro: false` though the plan says "pro" and the account may be unlimited, so neither the plan
//! nor /settings/usage-status says anything about the apps. The same licence and key as
//! PDF.dk Desktop.
//!
//! Checked in this order: the signature (key id k1), expires_at, pro, and the computer it names
//! (when it names one, it must be this one). The last good licence is kept for 7 days, so a
//! moment without pdf.dk (503, no network) doesn't close the hot folders.

use std::time::{SystemTime, UNIX_EPOCH};

/// pdf.dk's licence signing keys (Ed25519, raw 32 bytes, base64) by key id.
const LICENCE_KEYS: &[(&str, &str)] = &[("k1", "BHEMc06Wsna1SS1vWmKpOvpFMP4Pv06qgnzyOBf+mqk=")];

struct Signed {
    pro: bool,
    expires_at: u64,
    device: Option<String>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "2026-10-16T23:01:30+00:00" (or with Z) → seconds since 1970.
fn parse_time(t: &str) -> Option<u64> {
    let b = t.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[10] != b'T' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| t.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, sec) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    let rest = t[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let off = match rest.as_bytes().first() {
        Some(b'+') | Some(b'-') if rest.len() >= 6 => {
            let s: i64 = if rest.starts_with('-') { -1 } else { 1 };
            s * (rest[1..3].parse::<i64>().ok()? * 3600 + rest[4..6].parse::<i64>().ok()? * 60)
        }
        _ => 0,
    };
    // Days from the civil date (Howard Hinnant's algorithm).
    let (yy, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + sec - off).ok()
}

/// "base64url(payload).base64url(signature)": the signature over the payload's exact bytes first,
/// then the payload is read.
fn verify(licence: &str) -> Option<Signed> {
    use base64::Engine;
    let b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let (p, sig) = licence.trim().split_once('.')?;
    let payload = b64url.decode(p.trim_end_matches('=')).ok()?;
    let sig = b64url.decode(sig.trim_end_matches('=')).ok()?;
    let kid = serde_json::from_slice::<serde_json::Value>(&payload).ok()?["kid"].as_str()?.to_string();
    let key = LICENCE_KEYS.iter().find(|(id, _)| *id == kid)?.1;
    let key = base64::engine::general_purpose::STANDARD.decode(key).ok()?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key).verify(&payload, &sig).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    if v["v"].as_i64() != Some(1) {
        return None;
    }
    Some(Signed { pro: v["pro"].as_bool().unwrap_or(false), expires_at: parse_time(v["expires_at"].as_str()?)?, device: v["device"].as_str().map(String::from) })
}

/// Pro for the apps on this computer, by a licence string: signed, not expired, and – when it
/// names a computer – this one.
fn pro_by(licence: &str) -> bool {
    verify(licence).is_some_and(|l| now() < l.expires_at && l.pro && l.device.as_deref().is_none_or(|d| d == crate::api::device_id()))
}

fn cache_path() -> Option<std::path::PathBuf> {
    crate::config::app_dir().ok().map(|d| d.join("licence.txt"))
}

/// Asks pdf.dk. A 409 `device_limit` means the account has the apps, on another computer (the
/// seat panel says so and offers to move it). No answer: the licence kept from last time.
pub async fn pro(token: &str) -> bool {
    let r = crate::api::http_client().get(format!("{}/licence/client", crate::api::api_base())).bearer_auth(token).header("Accept", "application/json").send().await;
    if let Ok(r) = r {
        let status = r.status();
        let v: serde_json::Value = r.json().await.unwrap_or_default();
        if status == reqwest::StatusCode::CONFLICT && crate::api::field(&v, "code").as_str() == Some("device_limit") {
            return true;
        }
        if status.is_success() {
            if let Some(l) = v["data"]["licence"].as_str().filter(|l| verify(l).is_some()) {
                if let Some(p) = cache_path() {
                    let _ = std::fs::write(p, l);
                }
                return pro_by(l);
            }
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            forget();
            return false;
        }
    }
    cache_path().and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|l| pro_by(&l))
}

/// Signed out: the kept licence goes too.
pub fn forget() {
    if let Some(p) = cache_path() {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// pdf.dk's own example licence (signed with k1; expired 16.10.2026).
    const PDFDK_LICENCE: &str = "eyJ2IjoxLCJraWQiOiJrMSIsInVzZXJfaWQiOjEsImVtYWlsIjoiYWRtaW5AcGRmLmRrIiwicGxhbiI6InN1cGVyYWRtaW4iLCJwcm8iOnRydWUsImlzX3VubGltaXRlZCI6dHJ1ZSwiaXNzdWVkX2F0IjoiMjAyNi0xMC0wOVQyMzowODoxNSswMDowMCIsImV4cGlyZXNfYXQiOiIyMDI2LTEwLTE2VDIzOjA4OjE1KzAwOjAwIn0.xZ_EMy9NAoucXJgKt3bxNO_msXcSpauEk7X5M1YXB0iesZpnUb8f800VaHLH_nN0Lu7sWso9PdyMcDap-01dAQ";

    #[test]
    fn signature_and_expiry() {
        let l = verify(PDFDK_LICENCE).expect("pdf.dk's signature must hold");
        assert!(l.pro);
        assert_eq!(l.expires_at, parse_time("2026-10-16T23:08:15+00:00").unwrap());
        // expired: not Pro any more
        assert!(!pro_by(PDFDK_LICENCE));
        let (p, sig) = PDFDK_LICENCE.split_once('.').unwrap();
        let mut b = p.as_bytes().to_vec();
        b[40] = if b[40] == b'A' { b'B' } else { b'A' };
        assert!(verify(&format!("{}.{}", String::from_utf8(b).unwrap(), sig)).is_none());
        assert!(verify("not.a-licence").is_none());
    }
}
